use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::config::Config;
use crate::domain::{parse_number, slugify, AdrMeta};
use crate::repository::{idx_path, AdrRepository};

pub fn create_new_adr<R: AdrRepository>(
    repo: &R,
    cfg: &Config,
    title: &str,
    supersedes: Option<u32>,
) -> Result<AdrMeta> {
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
            .replace("{{STATUS}}", "Proposed")
            .replace(
                "{{SUPERSEDES}}",
                &supersedes.map(|n| format!("{:04}", n)).unwrap_or_default(),
            )
    } else {
        let mut header = format!(
            "# ADR {:04}: {}\n\nDate: {}\nStatus: Proposed\n",
            next, title, date
        );
        if let Some(n) = supersedes {
            header.push_str(&format!("Supersedes: {:04}\n", n));
        }
        header.push_str(
            "\n## Context\n\nDescribe the context and forces at play.\n\n## Decision\n\nState the decision that was made and why.\n\n## Consequences\n\nList the trade-offs and follow-ups.\n",
        );
        header
    };

    repo.write_string(&path, &content)?;

    let meta = AdrMeta {
        number: next,
        title: title.to_string(),
        status: "Proposed".to_string(),
        date: date.clone(),
        supersedes,
        superseded_by: None,
        path: path.clone(),
    };
    adrs.push(meta.clone());
    adrs.sort_by_key(|a| a.number);
    write_index(repo, cfg, &adrs)?;
    Ok(meta)
}

pub fn mark_superseded<R: AdrRepository>(
    repo: &R,
    cfg: &Config,
    old_number: u32,
    new_number: u32,
) -> Result<()> {
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
                        if num == old_number {
                            target_path = Some(path);
                            break;
                        }
                    }
                }
            }
        }
    }

    let Some(path) = target_path else {
        return Err(anyhow!("Could not find ADR {:04} to supersede", old_number));
    };

    let contents = repo.read_string(&path)?;
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
        let insert_at = lines
            .iter()
            .position(|l| l.trim().is_empty())
            .unwrap_or(lines.len());
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

pub fn accept<R: AdrRepository>(repo: &R, cfg: &Config, id_or_title: &str) -> Result<AdrMeta> {
    let adrs = repo.list()?;
    // Try by number, else by title (case-insensitive exact match)
    let target = match parse_number(id_or_title) {
        Ok(n) if adrs.iter().any(|a| a.number == n) => adrs
            .into_iter()
            .find(|a| a.number == n)
            .ok_or_else(|| anyhow!("ADR not found by id: {}", n))?,
        _ => {
            let lower = id_or_title.trim().to_ascii_lowercase();
            adrs.into_iter()
                .find(|a| a.title.to_ascii_lowercase() == lower)
                .ok_or_else(|| anyhow!("ADR not found by id or title: {}", id_or_title))?
        }
    };

    let mut content = repo.read_string(&target.path)?;
    let today = Local::now().format("%Y-%m-%d").to_string();
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut found_status = false;
    let mut found_date = false;
    for l in &mut lines {
        if l.starts_with("Status:") {
            *l = "Status: Accepted".to_string();
            found_status = true;
        }
        if l.starts_with("Date:") {
            *l = format!("Date: {}", today);
            found_date = true;
        }
    }
    if !found_status {
        // insert after header line
        let insert_at = if !lines.is_empty() { 1 } else { 0 };
        lines.insert(insert_at, "Status: Accepted".to_string());
    }
    if !found_date {
        lines.insert(1, format!("Date: {}", today));
    }
    content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }
    repo.write_string(&target.path, &content)?;

    // refresh index and return updated meta
    let adrs2 = repo.list()?;
    write_index(repo, cfg, &adrs2)?;
    let updated = adrs2
        .into_iter()
        .find(|a| a.number == target.number)
        .ok_or_else(|| anyhow!("Updated ADR not found"))?;
    Ok(updated)
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
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
        };

        let meta = create_new_adr(&repo, &cfg, "First Decision", None).unwrap();
        assert_eq!(meta.number, 1);
        assert!(meta.path.exists());
        assert_eq!(meta.status, "Proposed");
        let idx = cfg.adr_dir.join("index.md");
        assert!(idx.exists());
        let adrs = repo.list().unwrap();
        assert_eq!(adrs.len(), 1);
        assert_eq!(adrs[0].title, "First Decision");
        assert_eq!(adrs[0].status, "Proposed");
    }

    #[test]
    fn test_supersede_updates_old_adr() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
        };

        let old = create_new_adr(&repo, &cfg, "Choose X", None).unwrap();
        let new_meta = create_new_adr(&repo, &cfg, "Choose Y", Some(old.number)).unwrap();
        mark_superseded(&repo, &cfg, old.number, new_meta.number).unwrap();

        let old_path = cfg.adr_dir.join(format!(
            "{:04}-{}.md",
            old.number,
            crate::domain::slugify("Choose X")
        ));
        let contents = repo.read_string(&old_path).unwrap();
        assert!(contents.contains("Status: Superseded by 0002"));
        assert!(contents.contains("Superseded-by: 0002"));
    }

    #[test]
    fn test_accept_by_id_and_title() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
        };

        let m1 = create_new_adr(&repo, &cfg, "Adopt Z", None).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let updated1 = accept(&repo, &cfg, &format!("{}", m1.number)).unwrap();
        assert_eq!(updated1.status, "Accepted");
        let c1 = repo.read_string(&updated1.path).unwrap();
        assert!(c1.contains("Status: Accepted"));
        assert!(c1.contains(&format!("Date: {}", today)));

        let _m2 = create_new_adr(&repo, &cfg, "Pick W", None).unwrap();
        let updated2 = accept(&repo, &cfg, "Pick W").unwrap();
        assert_eq!(updated2.status, "Accepted");
    }

    #[test]
    fn test_next_number_after_gap() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        std::fs::create_dir_all(&adr_dir).unwrap();
        // Pre-create a higher numbered ADR to create a gap
        let pre = adr_dir.join("0005-existing.md");
        std::fs::write(&pre, "# ADR 0005: Existing\n\nBody\n").unwrap();

        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: None,
        };

        let meta = create_new_adr(&repo, &cfg, "Next After Gap", None).unwrap();
        assert_eq!(meta.number, 6);
        assert!(meta.path.ends_with("0006-next-after-gap.md"));
    }

    #[test]
    fn test_template_substitution_with_supersedes() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let tpl_path = dir.path().join("tpl.md");
        std::fs::write(
            &tpl_path,
            "# ADR {{NUMBER}}: {{TITLE}}\n\nDate: {{DATE}}\nStatus: {{STATUS}}\nSupersedes: {{SUPERSEDES}}\n\nBody\n",
        )
        .unwrap();

        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: Some(tpl_path.clone()),
        };
        let meta = create_new_adr(&repo, &cfg, "Use Template", Some(3)).unwrap();
        let content = repo.read_string(&meta.path).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(content.contains("# ADR 0001: Use Template"));
        assert!(content.contains(&format!("Date: {}", today)));
        assert!(content.contains("Status: Proposed"));
        assert!(content.contains("Supersedes: 0003"));
    }

    #[test]
    fn test_mark_superseded_inserts_when_missing() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        std::fs::create_dir_all(&adr_dir).unwrap();
        // Old ADR without status/superseded-by lines
        let old_path = adr_dir.join("0001-old.md");
        std::fs::write(&old_path, "# ADR 0001: Old\n\nContext\n").unwrap();
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: None,
        };

        // Create new ADR to get number 2
        let new_meta = create_new_adr(&repo, &cfg, "New", None).unwrap();
        mark_superseded(&repo, &cfg, 1, new_meta.number).unwrap();
        let updated = repo.read_string(&old_path).unwrap();
        assert!(updated.contains("Status: Superseded by 0002"));
        assert!(updated.contains("Superseded-by: 0002"));
    }

    #[test]
    fn test_accept_zero_padded_and_case_insensitive_title() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: None,
        };

        let m1 = create_new_adr(&repo, &cfg, "Choose DB", None).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let _ = accept(&repo, &cfg, "0001").unwrap();
        let c1 = repo.read_string(&m1.path).unwrap();
        assert!(c1.contains("Status: Accepted"));
        assert!(c1.contains(&format!("Date: {}", today)));

        let _m2 = create_new_adr(&repo, &cfg, "Use Queue", None).unwrap();
        let updated2 = accept(&repo, &cfg, "use queue").unwrap();
        assert_eq!(updated2.status, "Accepted");
    }
}
