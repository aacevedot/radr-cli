use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use clap::{Parser, Subcommand};
use regex::Regex;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name = "radr", about = "Manage Architecture Decision Records (ADRs)")]
struct Cli {
    /// Optional path to a config file (JSON, YAML, or TOML)
    #[arg(long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new ADR with a title
    New {
        /// Title for the ADR
        title: String,
        /// Initial status (default: Accepted)
        #[arg(long, default_value = "Accepted")]
        status: String,
    },
    /// Create a new ADR that supersedes an existing ADR number
    Supersede {
        /// ADR number to supersede (e.g., 0003 or 3)
        id: String,
        /// Title for the new ADR
        title: String,
    },
    /// List ADRs found in the ADR directory
    List,
    /// Regenerate the index.md file
    Index,
}

#[derive(Debug, Clone)]
struct Config {
    adr_dir: PathBuf,
    index_name: String,
    template: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            adr_dir: PathBuf::from("docs/adr"),
            index_name: "index.md".to_string(),
            template: None,
        }
    }
}

#[derive(Deserialize, Debug)]
struct FileConfig {
    adr_dir: Option<PathBuf>,
    index_name: Option<String>,
    template: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct AdrMeta {
    number: u32,
    title: String,
    status: String,
    date: String,
    supersedes: Option<u32>,
    superseded_by: Option<u32>,
    path: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = load_config(cli.config.as_ref())?;

    fs::create_dir_all(&cfg.adr_dir)
        .with_context(|| format!("Creating ADR directory at {}", cfg.adr_dir.display()))?;

    match cli.command {
        Commands::New { title, status } => {
            let meta = create_new_adr(&cfg, &title, &status, None)?;
            println!(
                "Created ADR {:04}: {} at {}",
                meta.number,
                meta.title,
                meta.path.display()
            );
        }
        Commands::Supersede { id, title } => {
            let old_num = parse_number(&id)?;
            let new_meta = create_new_adr(&cfg, &title, "Accepted", Some(old_num))?;
            mark_superseded(&cfg, old_num, new_meta.number)?;
            // Refresh index so the old ADR's updated status is listed
            let adrs = read_all_adrs(&cfg)?;
            write_index(&cfg, &adrs)?;
            println!(
                "Created ADR {:04} superseding {:04}",
                new_meta.number, old_num
            );
        }
        Commands::List | Commands::Index => {
            let adrs = read_all_adrs(&cfg)?;
            for a in &adrs {
                println!(
                    "{:04} | {} | {} | {}",
                    a.number, a.title, a.status, a.date
                );
            }
            write_index(&cfg, &adrs)?;
            println!(
                "Updated {}",
                cfg.adr_dir.join(&cfg.index_name).display()
            );
        }
    }

    Ok(())
}

fn load_config(cli_path: Option<&PathBuf>) -> Result<Config> {
    let mut cfg = Config::default();

    let path = if let Some(p) = cli_path {
        Some(p.clone())
    } else if let Ok(env_p) = env::var("RADR_CONFIG") {
        Some(PathBuf::from(env_p))
    } else {
        let candidates = [
            "radr.toml",
            "radr.yaml",
            "radr.yml",
            "radr.json",
            ".radrrc.toml",
            ".radrrc.yaml",
            ".radrrc.yml",
            ".radrrc.json",
        ];
        candidates
            .iter()
            .map(PathBuf::from)
            .find(|p| p.exists())
    };

    if let Some(p) = path {
        let ext = p.extension().and_then(OsStr::to_str).unwrap_or("");
        let contents = fs::read_to_string(&p)
            .with_context(|| format!("Reading config at {}", p.display()))?;
        let fc: FileConfig = match ext.to_ascii_lowercase().as_str() {
            "json" => serde_json::from_str(&contents)
                .with_context(|| format!("Parsing JSON config at {}", p.display()))?,
            "yaml" | "yml" => serde_yaml::from_str(&contents)
                .with_context(|| format!("Parsing YAML config at {}", p.display()))?,
            "toml" => toml::from_str(&contents)
                .with_context(|| format!("Parsing TOML config at {}", p.display()))?,
            other => return Err(anyhow!("Unsupported config extension: {}", other)),
        };

        if let Some(d) = fc.adr_dir { cfg.adr_dir = d; }
        if let Some(i) = fc.index_name { cfg.index_name = i; }
        if let Some(t) = fc.template { cfg.template = Some(t); }
    }

    Ok(cfg)
}

fn create_new_adr(cfg: &Config, title: &str, status: &str, supersedes: Option<u32>) -> Result<AdrMeta> {
    let mut adrs = read_all_adrs(cfg)?;
    let next = adrs.iter().map(|a| a.number).max().unwrap_or(0) + 1;
    let slug = slugify(title);
    let filename = format!("{:04}-{}.md", next, slug);
    let path = cfg.adr_dir.join(filename);
    let date = Local::now().format("%Y-%m-%d").to_string();

    let content = if let Some(tpl_path) = &cfg.template {
        let tpl = fs::read_to_string(tpl_path)
            .with_context(|| format!("Reading template at {}", tpl_path.display()))?;
        tpl.replace("{{NUMBER}}", &format!("{:04}", next))
            .replace("{{TITLE}}", title)
            .replace("{{DATE}}", &date)
            .replace("{{STATUS}}", status)
            .replace(
                "{{SUPERSEDES}}",
                &supersedes
                    .map(|n| format!("{:04}", n))
                    .unwrap_or_else(|| "".to_string()),
            )
    } else {
        let mut header = format!(
            "# ADR {:04}: {}\n\nDate: {}\nStatus: {}\n",
            next, title, date, status
        );
        if let Some(n) = supersedes { header.push_str(&format!("Supersedes: {:04}\n", n)); }
        header.push_str(
            "\n## Context\n\nDescribe the context and forces at play.\n\n## Decision\n\nState the decision that was made and why.\n\n## Consequences\n\nList the trade-offs and follow-ups.\n",
        );
        header
    };

    write_string(&path, &content)?;

    // Refresh and write index
    adrs.push(AdrMeta {
        number: next,
        title: title.to_string(),
        status: status.to_string(),
        date: date.clone(),
        supersedes,
        superseded_by: None,
        path: path.clone(),
    });
    adrs.sort_by_key(|a| a.number);
    write_index(cfg, &adrs)?;

    Ok(AdrMeta {
        number: next,
        title: title.to_string(),
        status: status.to_string(),
        date,
        supersedes,
        superseded_by: None,
        path,
    })
}

fn mark_superseded(cfg: &Config, old_number: u32, new_number: u32) -> Result<()> {
    let mut old_path = None;
    for entry in fs::read_dir(&cfg.adr_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) == Some("md") {
            if let Some(num) = number_from_filename(&path) {
                if num == old_number {
                    old_path = Some(path);
                    break;
                }
            }
        }
    }

    let Some(path) = old_path else {
        return Err(anyhow!("Could not find ADR {:04} to supersede", old_number));
    };

    let contents = fs::read_to_string(&path)?;
    let mut lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let mut found_status = false;
    let mut found_superseded_by = false;
    for l in &mut lines {
        if l.starts_with("Status:") {
            *l = format!("Status: Superseded by {:04}", new_number);
            found_status = true;
        }
        if l.starts_with("Superseded-by:") {
            *l = format!("Superseded-by: {:04}", new_number);
            found_superseded_by = true;
        }
    }
    if !found_status {
        lines.insert(1, format!("Status: Superseded by {:04}", new_number));
    }
    if !found_superseded_by {
        // Try to insert after status/date block
        let insert_at = lines
            .iter()
            .position(|l| l.trim().is_empty())
            .unwrap_or(lines.len());
        lines.insert(insert_at, format!("Superseded-by: {:04}", new_number));
    }
    let mut content = lines.join("\n");
    content.push('\n');
    write_string(&path, &content)
}

fn write_index(cfg: &Config, adrs: &[AdrMeta]) -> Result<()> {
    let mut content = String::new();
    content.push_str("# Architecture Decision Records\n\n");
    for a in adrs {
        let fname = a
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("");
        content.push_str(&format!(
            "- [{:04}: {}]({}) — Status: {} — Date: {}\n",
            a.number, a.title, fname, a.status, a.date
        ));
    }
    content.push('\n');
    let idx_path = cfg.adr_dir.join(&cfg.index_name);
    write_string(&idx_path, &content)
}

fn read_all_adrs(cfg: &Config) -> Result<Vec<AdrMeta>> {
    let mut res = Vec::new();
    let re = Regex::new(r"^\d{4}-.*\.md$").unwrap();
    for entry in fs::read_dir(&cfg.adr_dir).with_context(|| {
        format!("Reading ADR directory at {}", cfg.adr_dir.display())
    })? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("md") {
            continue;
        }
        let fname = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        if !re.is_match(fname) {
            continue;
        }
        let meta = parse_adr_file(&path)?;
        res.push(meta);
    }
    res.sort_by_key(|a| a.number);
    Ok(res)
}

fn parse_adr_file(path: &Path) -> Result<AdrMeta> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut number = number_from_filename(path).unwrap_or(0);
    let mut title = String::new();
    let mut status = String::from("Accepted");
    let mut date = String::new();
    let mut supersedes: Option<u32> = None;
    let mut superseded_by: Option<u32> = None;

    for (i, line) in reader.lines().take(200).enumerate() {
        let line = line?;
        if i == 0 {
            // Try to parse: # ADR 0001: Title
            if let Some(idx) = line.find(": ") {
                let head = &line[..idx];
                if let Some(num_idx) = head.rfind(' ') {
                    if let Ok(n) = head[num_idx + 1..].parse::<u32>() {
                        number = n;
                    }
                }
                title = line[idx + 2..].trim().to_string();
            }
        }
        if line.starts_with("Title:") {
            title = line[6..].trim().to_string();
        }
        if line.starts_with("Date:") {
            date = line[5..].trim().to_string();
        }
        if line.starts_with("Status:") {
            status = line[7..].trim().to_string();
        }
        if line.starts_with("Supersedes:") {
            let v = line[11..].trim();
            if let Ok(n) = v.parse::<u32>() { supersedes = Some(n); }
        }
        if line.starts_with("Superseded-by:") {
            let v = line[14..].trim();
            if let Ok(n) = v.parse::<u32>() { superseded_by = Some(n); }
        }
    }

    if title.is_empty() {
        title = title_from_filename(path).unwrap_or_else(|| "Untitled".to_string());
    }
    if date.is_empty() {
        date = Local::now().format("%Y-%m-%d").to_string();
    }

    Ok(AdrMeta {
        number,
        title,
        status,
        date,
        supersedes,
        superseded_by,
        path: path.to_path_buf(),
    })
}

fn write_string(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let mut f = File::create(path)?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if c.is_ascii_whitespace() || c == '-' || c == '_' {
            if !last_dash {
                out.push('-');
                last_dash = true;
            }
        }
        // ignore other punctuation
    }
    while out.ends_with('-') { out.pop(); }
    while out.starts_with('-') { out.remove(0); }
    if out.is_empty() { "adr".to_string() } else { out }
}

fn number_from_filename(path: &Path) -> Option<u32> {
    let fname = path.file_name()?.to_str()?;
    let re = Regex::new(r"^(\d{4})-").ok()?;
    let caps = re.captures(fname)?;
    caps.get(1)?.as_str().parse::<u32>().ok()
}

fn title_from_filename(path: &Path) -> Option<String> {
    let fname = path.file_stem()?.to_str()?; // e.g., 0001-my-title
    let mut parts = fname.splitn(2, '-');
    parts.next()?; // skip number
    let slug = parts.next().unwrap_or("");
    if slug.is_empty() { return None; }
    let title = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    Some(title)
}

fn parse_number(s: &str) -> Result<u32> {
    let s = s.trim();
    let s = s.trim_start_matches('0');
    if s.is_empty() { Ok(0) } else { Ok(s.parse::<u32>()?) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
        assert_eq!(slugify("Caps_and-Dashes"), "caps-and-dashes");
        assert_eq!(slugify("@#Weird!! Title??"), "weird-title");
        assert_eq!(slugify(""), "adr");
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_number("0003").unwrap(), 3);
        assert_eq!(parse_number("3").unwrap(), 3);
        assert_eq!(parse_number("0000").unwrap(), 0);
    }

    #[test]
    fn test_create_new_adr_and_index() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let cfg = Config { adr_dir: adr_dir.clone(), index_name: "index.md".to_string(), template: None };

        let meta = create_new_adr(&cfg, "First Decision", "Accepted", None).unwrap();
        assert_eq!(meta.number, 1);
        assert!(meta.path.exists());

        let idx = adr_dir.join("index.md");
        assert!(idx.exists());

        let adrs = read_all_adrs(&cfg).unwrap();
        assert_eq!(adrs.len(), 1);
        assert_eq!(adrs[0].title, "First Decision");
    }

    #[test]
    fn test_supersede_updates_old_adr() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let cfg = Config { adr_dir: adr_dir.clone(), index_name: "index.md".to_string(), template: None };

        let old = create_new_adr(&cfg, "Choose X", "Accepted", None).unwrap();
        assert_eq!(old.number, 1);
        let new_meta = create_new_adr(&cfg, "Choose Y", "Accepted", Some(old.number)).unwrap();
        assert_eq!(new_meta.number, 2);

        // Mark old superseded by new
        mark_superseded(&cfg, old.number, new_meta.number).unwrap();

        let old_path = cfg.adr_dir.join(format!("{:04}-{}.md", old.number, slugify("Choose X")));
        let contents = fs::read_to_string(&old_path).unwrap();
        assert!(contents.contains("Status: Superseded by 0002"));
        assert!(contents.contains("Superseded-by: 0002"));
    }
}

