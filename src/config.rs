use std::{env, ffi::OsStr, fs, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Config {
    pub adr_dir: PathBuf,
    pub index_name: String,
    pub template: Option<PathBuf>,
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

pub fn load_config(cli_path: Option<&PathBuf>) -> Result<Config> {
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
        candidates.iter().map(PathBuf::from).find(|p| p.exists())
    };

    if let Some(p) = path {
        let ext = p.extension().and_then(OsStr::to_str).unwrap_or("");
        let contents =
            fs::read_to_string(&p).with_context(|| format!("Reading config at {}", p.display()))?;
        let fc: FileConfig = match ext.to_ascii_lowercase().as_str() {
            "json" => serde_json::from_str(&contents)
                .with_context(|| format!("Parsing JSON config at {}", p.display()))?,
            "yaml" | "yml" => serde_yaml::from_str(&contents)
                .with_context(|| format!("Parsing YAML config at {}", p.display()))?,
            "toml" => toml::from_str(&contents)
                .with_context(|| format!("Parsing TOML config at {}", p.display()))?,
            other => return Err(anyhow!("Unsupported config extension: {}", other)),
        };

        if let Some(d) = fc.adr_dir {
            cfg.adr_dir = d;
        }
        if let Some(i) = fc.index_name {
            cfg.index_name = i;
        }
        if let Some(t) = fc.template {
            cfg.template = Some(t);
        }
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let c = Config::default();
        assert_eq!(c.index_name, "index.md");
    }

    #[test]
    fn test_load_from_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("radr.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "adr_dir='adrs'\nindex_name='IDX.md'").unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let cfg = load_config(None).unwrap();
        assert_eq!(cfg.adr_dir, PathBuf::from("adrs"));
        assert_eq!(cfg.index_name, "IDX.md");
    }
}
