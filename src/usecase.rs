use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::config::Config;
use crate::domain::{slugify, AdrMeta};
use crate::repository::{AdrRepository, idx_path};

pub fn create_new_adr<R: AdrRepository>(repo: &R, cfg: &Config, title: &str, status: &str, supersedes: Option<u32>) -> Result<AdrMeta> {
    let mut adrs = repo.list()?;
    let next = adrs.iter().map(|a| a.number).max().unwrap_or(0) + 1;
    let slug = slugify(title);
    let filename = format!("{:04}-{}.md", next, slug);
    let path = repo.adr_dir().join(filename);
    let date = Local::now().format("%Y-%m-%d").to_string();

    let content = if let Some(tpl_path) = &cfg.template {
        let tpl = std::fs::read_to_string(tpl_path)
            .with_context(|| format!("Reading template at {}", tpl_path.display()))?;
        tpl.replace("{{NUMBER}}", &format!("{:04}", next))
            .replace("{{TITLE}}", title)
            .replace("{{DATE}}", &date)
            .replace("{{STATUS}}", status)
            .replace("{{SUPERSEDES}}", &supersedes.map(|n| format!("{:04}", n)).unwrap_or_else(|| "".to_string()))
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

    repo.write_string(&path, &content)?;

    let meta = AdrMeta { number: next, title: title.to_string(), status: status.to_string(), date: date.clone(), supersedes, superseded_by: None, path: path.clone() };
    adrs.push(meta.clone());
    adrs.sort_by_key(|a| a.number);
    write_index(repo, cfg, &adrs)?;
    Ok(meta)
}

pub fn mark_superseded<R: AdrRepository>(repo: &R, cfg: &Config, old_number: u32, new_number: u32) -> Result<()> {
    // Find old ADR by number
    let mut target_path: Option<PathBuf> = None;
    for entry in std::fs::read_dir(repo.adr_dir())? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) == Some("md") {
            // extract number from filename
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some(num_part) = stem.split('-').next() {
                    if let Ok(num) = num_part.parse::<u32>() {
                        if num == old_number { target_path = Some(path); break; }
                    }
                }
            }
        }
    }

    let Some(path) = target_path else { return Err(anyhow!("Could not find ADR {:04} to supersede", old_number)); };

    let contents = repo.read_string(&path)?;
    let mut lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
    let mut found_status = false;
    let mut found_superseded_by = false;
    for l in &mut lines {
        if l.starts_with("Status:") { *l = format!("Status: Superseded by {:04}", new_number); found_status = true; }
        if l.starts_with("Superseded-by:") { *l = format!("Superseded-by: {:04}", new_number); found_superseded_by = true; }
    }
    if !found_status { lines.insert(1, format!("Status: Superseded by {:04}", new_number)); }
    if !found_superseded_by {
        let insert_at = lines.iter().position(|l| l.trim().is_empty()).unwrap_or(lines.len());
        lines.insert(insert_at, format!("Superseded-by: {:04}", new_number));
    }
    let mut content = lines.join("\n");
    content.push('\n');
    repo.write_string(&path, &content)?;

    // refresh index
    let adrs = repo.list()?;
    write_index(repo, cfg, &adrs)?;
    Ok(())
}

pub fn list_and_index<R: AdrRepository>(repo: &R, cfg: &Config) -> Result<Vec<AdrMeta>> {
    let adrs = repo.list()?;
    write_index(repo, cfg, &adrs)?;
    Ok(adrs)
}

fn write_index<R: AdrRepository>(repo: &R, cfg: &Config, adrs: &[AdrMeta]) -> Result<()> {
    let mut content = String::new();
    content.push_str("# Architecture Decision Records\n\n");
    for a in adrs {
        let fname = a.path.file_name().and_then(OsStr::to_str).unwrap_or("");
        content.push_str(&format!(
            "- [{:04}: {}]({}) — Status: {} — Date: {}\n",
            a.number, a.title, fname, a.status, a.date
        ));
    }
    content.push('\n');
    let idx = idx_path(&cfg.adr_dir, &cfg.index_name);
    repo.write_string(&idx, &content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::fs::FsAdrRepository;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_index() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config { adr_dir: adr_dir.clone(), index_name: "index.md".to_string(), template: None };

        let meta = create_new_adr(&repo, &cfg, "First Decision", "Accepted", None).unwrap();
        assert_eq!(meta.number, 1);
        assert!(meta.path.exists());
        let idx = cfg.adr_dir.join("index.md");
        assert!(idx.exists());
        let adrs = repo.list().unwrap();
        assert_eq!(adrs.len(), 1);
        assert_eq!(adrs[0].title, "First Decision");
    }

    #[test]
    fn test_supersede_updates_old_adr() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config { adr_dir: adr_dir.clone(), index_name: "index.md".to_string(), template: None };

        let old = create_new_adr(&repo, &cfg, "Choose X", "Accepted", None).unwrap();
        let new_meta = create_new_adr(&repo, &cfg, "Choose Y", "Accepted", Some(old.number)).unwrap();
        mark_superseded(&repo, &cfg, old.number, new_meta.number).unwrap();

        let old_path = cfg.adr_dir.join(format!("{:04}-{}.md", old.number, crate::domain::slugify("Choose X")));
        let contents = repo.read_string(&old_path).unwrap();
        assert!(contents.contains("Status: Superseded by 0002"));
        assert!(contents.contains("Superseded-by: 0002"));
    }
}

