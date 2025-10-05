use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::domain::AdrMeta;

pub mod fs;

pub trait AdrRepository {
    fn adr_dir(&self) -> &Path;
    fn list(&self) -> Result<Vec<AdrMeta>>;
    fn read_string(&self, path: &Path) -> Result<String>;
    fn write_string(&self, path: &Path, content: &str) -> Result<()>;
}

pub fn idx_path(dir: &Path, index_name: &str) -> PathBuf {
    dir.join(index_name)
}

