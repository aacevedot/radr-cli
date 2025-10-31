use anyhow::{anyhow, Context, Result};
use chrono::Local;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::config::Config;
use crate::domain::{parse_number, slugify, AdrMeta};
use crate::repository::{idx_path, AdrRepository};
use crate::yaml_util::escape_yaml;
use std::collections::HashMap;

pub fn create_new_adr<R: AdrRepository>(
    repo: &R,
    cfg: &Config,
    title: &str,
    supersedes: Option<u32>,
) -> Result<AdrMeta> {
    let mut adrs = repo.list()?;
    let next = adrs.iter().map(|a| a.number).max().unwrap_or(0) + 1;
    let slug = slugify(title);
    let ext = cfg.format.as_str();
    let filename = format!("{:04}-{}.{}", next, slug, ext);
    let path = repo.adr_dir().join(filename);
    let date = Local::now().format("%Y-%m-%d").to_string();

    // Resolve supersedes display: link to existing ADR filename when possible
    let supersedes_display = supersedes.map(|n| {
        if let Some(fname) = adrs
            .iter()
            .find(|a| a.number == n)
            .and_then(|a| a.path.file_name().and_then(OsStr::to_str))
        {
            format!("[{:04}]({})", n, fname)
        } else {
            format!("{:04}", n)
        }
    });

    let content = if let Some(tpl_path) = &cfg.template {
        let tpl = std::fs::read_to_string(tpl_path)
            .with_context(|| format!("Reading template at {}", tpl_path.display()))?;
        tpl.replace("{{NUMBER}}", &format!("{:04}", next))
            .replace("{{TITLE}}", title)
            .replace("{{DATE}}", &date)
            .replace("{{STATUS}}", "Proposed")
            .replace(
                "{{SUPERSEDES}}",
                supersedes_display.as_deref().unwrap_or_default(),
            )
    } else if cfg.front_matter {
        let mut body = String::new();
        body.push_str("---\n");
        body.push_str(&format!("title: {}\n", escape_yaml(title)));
        body.push_str("---\n\n");
        body.push_str(&format!("Date: {}\n", date));
        body.push_str("Status: Proposed\n");
        if let Some(sup) = &supersedes_display {
            body.push_str(&format!("Supersedes: {}\n", sup));
        }
        body.push('\n');
        body.push_str("## Context\n\nDescribe the context and forces at play.\n\n");
        body.push_str("## Decision\n\nState the decision that was made and why.\n\n");
        body.push_str("## Consequences\n\nList the trade-offs and follow-ups.\n");
        body
    } else {
        let mut header = format!(
            "# ADR {:04}: {}\n\nDate: {}\nStatus: Proposed\n",
            next, title, date
        );
        if let Some(sup) = &supersedes_display {
            header.push_str(&format!("Supersedes: {}\n", sup));
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
        date,
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
    // Locate ADR by listing metadata to be robust even if dir missing
    let adrs = repo.list()?;
    let path: PathBuf = adrs
        .into_iter()
        .find(|a| a.number == old_number)
        .map(|a| a.path)
        .ok_or_else(|| anyhow!("Could not find ADR {:04} to supersede", old_number))?;

    let contents = repo.read_string(&path)?;
    let mut updated = String::new();
    if let Some(stripped) = contents.strip_prefix("---\n") {
        // Front matter present: keep it as-is, update fields in body
        if let Some(end) = stripped.find("\n---\n") {
            let fm_block = &stripped[..end];
            let rest = &stripped[end + 5..];
            let mut lines: Vec<String> = rest.lines().map(|s| s.to_string()).collect();
            // Update status/superseded-by with ordering
            let mut idx_status: Option<usize> = None;
            let mut idx_superseded_by: Option<usize> = None;
            for (i, l) in lines.iter_mut().enumerate() {
                if l.starts_with("Status:") {
                    *l = format!("Status: Superseded by {:04}", new_number);
                    idx_status = Some(i);
                }
                if l.starts_with("Superseded-by:") {
                    *l = format!("Superseded-by: {:04}", new_number);
                    idx_superseded_by = Some(i);
                }
            }
            if idx_status.is_none() {
                let insert_at = 0; // top of body
                lines.insert(
                    insert_at,
                    format!("Status: Superseded by {:04}", new_number),
                );
                idx_status = Some(insert_at);
            }
            match (idx_status, idx_superseded_by) {
                (Some(s_idx), Some(sb_idx)) => {
                    let desired = s_idx + 1;
                    if sb_idx != desired {
                        let _ = lines.remove(sb_idx);
                        let insert_pos = if sb_idx < desired {
                            desired - 1
                        } else {
                            desired
                        };
                        lines.insert(insert_pos, format!("Superseded-by: {:04}", new_number));
                    }
                }
                (Some(s_idx), None) => {
                    lines.insert(s_idx + 1, format!("Superseded-by: {:04}", new_number));
                }
                _ => {}
            }

            updated.push_str("---\n");
            updated.push_str(fm_block);
            updated.push_str("\n---\n");
            if !rest.starts_with('\n') && (lines.first().map(|l| !l.is_empty()).unwrap_or(false)) {
                updated.push('\n');
            }
            updated.push_str(&lines.join("\n"));
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
        } else {
            updated = contents;
        }
    } else {
        let mut lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();
        let mut idx_status: Option<usize> = None;
        let mut idx_superseded_by: Option<usize> = None;
        for (i, l) in lines.iter_mut().enumerate() {
            if l.starts_with("Status:") {
                *l = format!("Status: Superseded by {:04}", new_number);
                idx_status = Some(i);
            }
            if l.starts_with("Superseded-by:") {
                *l = format!("Superseded-by: {:04}", new_number);
                idx_superseded_by = Some(i);
            }
        }
        if idx_status.is_none() {
            let insert_at = if !lines.is_empty() { 1 } else { 0 };
            lines.insert(
                insert_at,
                format!("Status: Superseded by {:04}", new_number),
            );
            idx_status = Some(insert_at);
        }
        // Ensure Superseded-by appears immediately after Status
        match (idx_status, idx_superseded_by) {
            (Some(s_idx), Some(sb_idx)) => {
                let desired = s_idx + 1;
                if sb_idx != desired {
                    // Remove current and insert at desired (adjust if removing before desired)
                    let _ = lines.remove(sb_idx);
                    let insert_pos = if sb_idx < desired {
                        desired - 1
                    } else {
                        desired
                    };
                    lines.insert(insert_pos, format!("Superseded-by: {:04}", new_number));
                }
            }
            (Some(s_idx), None) => {
                lines.insert(s_idx + 1, format!("Superseded-by: {:04}", new_number));
            }
            _ => {}
        }

        updated = lines.join("\n");
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
    }
    repo.write_string(&path, &updated)?;

    // refresh index
    let adrs = repo.list()?;
    write_index(repo, cfg, &adrs)?;
    Ok(())
}

pub fn reformat<R: AdrRepository>(repo: &R, cfg: &Config, id: u32) -> Result<AdrMeta> {
    let adrs = repo.list()?;
    let target = adrs
        .iter()
        .find(|a| a.number == id)
        .ok_or_else(|| anyhow!("ADR not found by id: {:04}", id))?;

    let original = repo.read_string(&target.path)?;

    // Build map for linking by number
    let mut by_number: HashMap<u32, String> = HashMap::new();
    for a in &adrs {
        if let Some(fname) = a.path.file_name().and_then(OsStr::to_str) {
            by_number.insert(a.number, fname.to_string());
        }
    }

    // Extract body content after any header/front-matter + meta lines
    fn body_after_meta(raw: &str) -> String {
        let mut rest = raw;
        if let Some(stripped) = raw.strip_prefix("---\n") {
            if let Some(end) = stripped.find("\n---\n") {
                rest = &stripped[end + 5..];
            }
        }
        let lines: Vec<&str> = rest.lines().collect();
        let mut i = 0usize;
        if i < lines.len() && lines[i].starts_with("# ADR ") {
            i += 1;
            if i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
        }
        while i < lines.len() {
            let l = lines[i];
            let is_meta = l.starts_with("Title:")
                || l.starts_with("Date:")
                || l.starts_with("Status:")
                || l.starts_with("Supersedes:")
                || l.starts_with("Superseded-by:");
            if is_meta {
                i += 1;
                continue;
            }
            if l.trim().is_empty() {
                i += 1;
                break;
            }
            break;
        }
        let tail = lines[i..].join("\n");
        if tail.is_empty() { String::new() } else { format!("{}\n", tail) }
    }

    let tail_body = body_after_meta(&original);

    // Render new content according to cfg
    let mut new_content = String::new();
    if cfg.front_matter {
        new_content.push_str("---\n");
        new_content.push_str(&format!("title: {}\n", escape_yaml(&target.title)));
        new_content.push_str("---\n\n");
        new_content.push_str(&format!("Date: {}\n", target.date));
        new_content.push_str(&format!("Status: {}\n", target.status));
        if let Some(n) = target.superseded_by {
            new_content.push_str(&format!("Superseded-by: {:04}\n", n));
        }
        if let Some(n) = target.supersedes {
            if let Some(fname) = by_number.get(&n) {
                new_content.push_str(&format!("Supersedes: [{:04}]({})\n", n, fname));
            } else {
                new_content.push_str(&format!("Supersedes: {:04}\n", n));
            }
        }
        new_content.push('\n');
        new_content.push_str(&tail_body);
    } else {
        new_content.push_str(&format!(
            "# ADR {:04}: {}\n\n",
            target.number, target.title
        ));
        new_content.push_str(&format!("Date: {}\n", target.date));
        new_content.push_str(&format!("Status: {}\n", target.status));
        if let Some(n) = target.superseded_by {
            new_content.push_str(&format!("Superseded-by: {:04}\n", n));
        }
        if let Some(n) = target.supersedes {
            if let Some(fname) = by_number.get(&n) {
                new_content.push_str(&format!("Supersedes: [{:04}]({})\n", n, fname));
            } else {
                new_content.push_str(&format!("Supersedes: {:04}\n", n));
            }
        }
        new_content.push('\n');
        new_content.push_str(&tail_body);
    }

    // Determine new path
    let slug = slugify(&target.title);
    let ext = cfg.format.as_str();
    let new_filename = format!("{:04}-{}.{}", target.number, slug, ext);
    let new_path = repo.adr_dir().join(new_filename);

    repo.write_string(&new_path, &new_content)?;

    // Remove old file if different path
    if new_path != target.path {
        let _ = std::fs::remove_file(&target.path);
    }

    // Update incoming links in other ADRs' Supersedes lines to point to the new filename
    let new_filename = new_path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("")
        .to_string();
    let mut adrs_scan = repo.list()?;
    for a in &mut adrs_scan {
        if a.number == id {
            continue;
        }
        let content = repo.read_string(&a.path)?;
        let mut changed = false;
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        for l in &mut lines {
            if l.starts_with("Supersedes: [") {
                // Try to parse number between [ and ]
                if let Some(lb) = l.find('[') {
                    if let Some(rb) = l[lb + 1..].find(']') {
                        let num_str = &l[lb + 1..lb + 1 + rb];
                        if let Ok(n) = num_str.parse::<u32>() {
                            if n == id {
                                *l = format!("Supersedes: [{:04}]({})", n, new_filename);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if changed {
            let mut out = lines.join("\n");
            if !out.ends_with('\n') {
                out.push('\n');
            }
            repo.write_string(&a.path, &out)?;
        }
    }

    // Refresh index and return updated meta
    let adrs2 = repo.list()?;
    write_index(repo, cfg, &adrs2)?;
    let updated = adrs2
        .into_iter()
        .find(|a| a.number == target.number)
        .ok_or_else(|| anyhow!("Reformatted ADR not found"))?;
    Ok(updated)
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
    if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(end) = stripped.find("\n---\n") {
            let fm_block = &stripped[..end];
            let rest = &stripped[end + 5..];
            let mut lines: Vec<String> = rest.lines().map(|s| s.to_string()).collect();
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
                lines.insert(0, "Status: Accepted".to_string());
            }
            if !found_date {
                lines.insert(0, format!("Date: {}", today));
            }
            let mut out = String::new();
            out.push_str("---\n");
            out.push_str(fm_block);
            out.push_str("\n---\n");
            out.push_str(&lines.join("\n"));
            out.push('\n');
            content = out;
        }
    } else {
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

pub fn reject<R: AdrRepository>(repo: &R, cfg: &Config, id_or_title: &str) -> Result<AdrMeta> {
    let adrs = repo.list()?;
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
    if let Some(stripped) = content.strip_prefix("---\n") {
        if let Some(end) = stripped.find("\n---\n") {
            let fm_block = &stripped[..end];
            let rest = &stripped[end + 5..];
            let mut lines: Vec<String> = rest.lines().map(|s| s.to_string()).collect();
            let mut found_status = false;
            let mut found_date = false;
            for l in &mut lines {
                if l.starts_with("Status:") {
                    *l = "Status: Rejected".to_string();
                    found_status = true;
                }
                if l.starts_with("Date:") {
                    *l = format!("Date: {}", today);
                    found_date = true;
                }
            }
            if !found_status {
                lines.insert(0, "Status: Rejected".to_string());
            }
            if !found_date {
                lines.insert(0, format!("Date: {}", today));
            }
            let mut out = String::new();
            out.push_str("---\n");
            out.push_str(fm_block);
            out.push_str("\n---\n");
            out.push_str(&lines.join("\n"));
            out.push('\n');
            content = out;
        }
    } else {
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut found_status = false;
        let mut found_date = false;
        for l in &mut lines {
            if l.starts_with("Status:") {
                *l = "Status: Rejected".to_string();
                found_status = true;
            }
            if l.starts_with("Date:") {
                *l = format!("Date: {}", today);
                found_date = true;
            }
        }
        if !found_status {
            let insert_at = if !lines.is_empty() { 1 } else { 0 };
            lines.insert(insert_at, "Status: Rejected".to_string());
        }
        if !found_date {
            lines.insert(1, format!("Date: {}", today));
        }
        content = lines.join("\n");
        if !content.ends_with('\n') {
            content.push('\n');
        }
    }
    repo.write_string(&target.path, &content)?;

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
    // Build map from number -> filename for linking
    let mut by_number: HashMap<u32, String> = HashMap::new();
    for a in adrs {
        if let Some(fname) = a.path.file_name().and_then(OsStr::to_str) {
            by_number.insert(a.number, fname.to_string());
        }
    }
    for a in adrs {
        let fname = a.path.file_name().and_then(OsStr::to_str).unwrap_or("");
        let status_display = if let Some(n) = a.superseded_by {
            if let Some(target) = by_number.get(&n) {
                format!("Superseded by [{:04}]({})", n, target)
            } else {
                format!("Superseded by {:04}", n)
            }
        } else {
            a.status.clone()
        };
        content.push_str(&format!(
            "- [{:04}: {}]({}) — Status: {} — Date: {}\n",
            a.number, a.title, fname, status_display, a.date
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
            ..Config::default()
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
            ..Config::default()
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
        // Ensure Superseded-by appears right after Status
        let pos_status = contents.find("Status: Superseded by 0002").unwrap();
        let pos_sb = contents.find("Superseded-by: 0002").unwrap();
        assert!(pos_status < pos_sb);
    }

    #[test]
    fn test_index_links_to_superseding_adr() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
            ..Config::default()
        };

        let old = create_new_adr(&repo, &cfg, "Choose X", None).unwrap();
        let new_meta = create_new_adr(&repo, &cfg, "Choose Y", Some(old.number)).unwrap();
        mark_superseded(&repo, &cfg, old.number, new_meta.number).unwrap();

        let index = cfg.adr_dir.join("index.md");
        let idx = repo.read_string(&index).unwrap();
        // Ensure the old ADR's status contains a link to the new ADR file
        assert!(idx.contains("Status: Superseded by [0002](0002-choose-y.md)"));
    }

    #[test]
    fn test_create_new_mdx_with_front_matter() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let mut cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: None,
            ..Config::default()
        };
        cfg.format = "mdx".into();
        cfg.front_matter = true;

        let meta = create_new_adr(&repo, &cfg, "Front Matter Title", None).unwrap();
        assert!(meta.path.ends_with("0001-front-matter-title.mdx"));
        let c = repo.read_string(&meta.path).unwrap();
        assert!(c.starts_with("---\n"));
        assert!(c.contains("title:"));
        assert!(c.contains("Status: Proposed"));
        assert!(c.contains("Date:"));
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
            ..Config::default()
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
    fn test_mark_superseded_not_found_errors() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
            ..Config::default()
        };
        // No ADR 0001 exists, should error
        let err = mark_superseded(&repo, &cfg, 1, 2).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Could not find ADR 0001"));
    }

    #[test]
    fn test_accept_not_found_errors() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".to_string(),
            template: None,
            ..Config::default()
        };
        let err = accept(&repo, &cfg, "999").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("ADR not found"));
    }

    #[test]
    fn test_create_with_missing_template_errors() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: Some(dir.path().join("missing.tpl")),
            ..Config::default()
        };
        let err = create_new_adr(&repo, &cfg, "X", None).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Reading template"));
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
            ..Config::default()
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
            ..Config::default()
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
            ..Config::default()
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
            ..Config::default()
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

    #[test]
    fn test_reject_by_id_and_title() {
        let dir = tempdir().unwrap();
        let adr_dir = dir.path().join("adrs");
        let repo = FsAdrRepository::new(&adr_dir);
        let cfg = Config {
            adr_dir: adr_dir.clone(),
            index_name: "index.md".into(),
            template: None,
            ..Config::default()
        };

        let m1 = create_new_adr(&repo, &cfg, "Reject Me", None).unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let updated1 = reject(&repo, &cfg, &format!("{}", m1.number)).unwrap();
        assert_eq!(updated1.status, "Rejected");
        let c1 = repo.read_string(&updated1.path).unwrap();
        assert!(c1.contains("Status: Rejected"));
        assert!(c1.contains(&format!("Date: {}", today)));

        let _m2 = create_new_adr(&repo, &cfg, "Another One", None).unwrap();
        let updated2 = reject(&repo, &cfg, "another one").unwrap();
        assert_eq!(updated2.status, "Rejected");
    }
}
