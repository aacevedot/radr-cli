use anyhow::{anyhow, Context, Result};
use chrono::Local;
use regex::Regex;
use std::{
    ffi::OsStr,
    fs,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use super::AdrRepository;
use crate::domain::AdrMeta;

pub struct FsAdrRepository {
    root: PathBuf,
}

impl FsAdrRepository {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self { root: root.into() }
    }

    fn parse_adr_file(&self, path: &Path) -> Result<AdrMeta> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut number = self.number_from_filename(path).unwrap_or(0);
        let mut title = String::new();
        let mut status = String::from("Accepted");
        let mut date = String::new();
        let mut supersedes: Option<u32> = None;
        let mut superseded_by: Option<u32> = None;

        for (i, line) in reader.lines().take(200).enumerate() {
            let line = line?;
            if i == 0 {
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
            if let Some(stripped) = line.strip_prefix("Title:") {
                title = stripped.trim().to_string();
            }
            if let Some(stripped) = line.strip_prefix("Date:") {
                date = stripped.trim().to_string();
            }
            if let Some(stripped) = line.strip_prefix("Status:") {
                status = stripped.trim().to_string();
            }
            if let Some(stripped) = line.strip_prefix("Supersedes:") {
                let v = stripped.trim();
                if let Ok(n) = v.parse::<u32>() {
                    supersedes = Some(n);
                }
            }
            if let Some(stripped) = line.strip_prefix("Superseded-by:") {
                let v = stripped.trim();
                if let Ok(n) = v.parse::<u32>() {
                    superseded_by = Some(n);
                }
            }
        }

        if title.is_empty() {
            title = self
                .title_from_filename(path)
                .unwrap_or_else(|| "Untitled".to_string());
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

    fn number_from_filename(&self, path: &Path) -> Option<u32> {
        let fname = path.file_name()?.to_str()?;
        let re = Regex::new(r"^(\d{4})-").ok()?;
        let caps = re.captures(fname)?;
        caps.get(1)?.as_str().parse::<u32>().ok()
    }

    fn title_from_filename(&self, path: &Path) -> Option<String> {
        let fname = path.file_stem()?.to_str()?;
        let mut parts = fname.splitn(2, '-');
        parts.next()?;
        let slug = parts.next().unwrap_or("");
        if slug.is_empty() {
            return None;
        }
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
}

impl AdrRepository for FsAdrRepository {
    fn adr_dir(&self) -> &Path {
        &self.root
    }

    fn list(&self) -> Result<Vec<AdrMeta>> {
        let mut res = Vec::new();
        if !self.root.exists() {
            return Ok(res);
        }
        let re = Regex::new(r"^\d{4}-.*\.md$")
            .map_err(|e| anyhow!("invalid ADR filename regex: {}", e))?;
        for entry in fs::read_dir(&self.root)
            .with_context(|| format!("Reading ADR directory at {}", self.root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(OsStr::to_str) != Some("md") {
                continue;
            }
            let fname = path.file_name().and_then(OsStr::to_str).unwrap_or("");
            if !re.is_match(fname) {
                continue;
            }
            let meta = self.parse_adr_file(&path)?;
            res.push(meta);
        }
        res.sort_by_key(|a| a.number);
        Ok(res)
    }

    fn read_string(&self, path: &Path) -> Result<String> {
        let content = fs::read_to_string(path)?;
        Ok(content)
    }

    fn write_string(&self, path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = File::create(path)?;
        f.write_all(content.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_empty_list_ok() {
        let dir = tempdir().unwrap();
        let repo = FsAdrRepository::new(dir.path());
        let list = repo.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_ignores_non_matching_and_fallbacks() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        // Non-matching files are ignored
        std::fs::write(root.join("README.md"), "hello").unwrap();
        // Minimal ADR with only filename, parser should fallback
        let adr_path = root.join("0007-no-status.md");
        std::fs::write(&adr_path, "# minimal file\n\nBody\n").unwrap();

        let repo = FsAdrRepository::new(root);
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        let a = &list[0];
        assert_eq!(a.number, 7);
        assert_eq!(a.title, "No Status");
        // Status defaults to Accepted when missing
        assert_eq!(a.status, "Accepted");
        // Date defaults to today when missing
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(a.date, today);
    }
}
