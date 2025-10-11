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

    #[test]
    fn test_cli_over_env_precedence_and_template() {
        let dir = tempdir().unwrap();
        let json = dir.path().join("radr.json");
        let yaml = dir.path().join("radr.yaml");
        let tpl = dir.path().join("tpl.md");
        std::fs::write(&tpl, "T").unwrap();
        std::fs::write(&json, b"{\n  \"adr_dir\": \"cli_adrs\",\n  \"index_name\": \"CLI.md\",\n  \"template\": \"tpl.md\"\n}\n").unwrap();
        std::fs::write(&yaml, b"adr_dir: env_adrs\nindex_name: ENV.md\n").unwrap();
        // Set env to YAML, but pass CLI JSON path; CLI should win
        std::env::set_var("RADR_CONFIG", &yaml);
        let cfg = load_config(Some(&json)).unwrap();
        assert_eq!(cfg.adr_dir, PathBuf::from("cli_adrs"));
        assert_eq!(cfg.index_name, "CLI.md");
        assert_eq!(
            cfg.template.as_deref(),
            Some(PathBuf::from("tpl.md").as_path())
        );
        std::env::remove_var("RADR_CONFIG");
    }

    #[test]
    fn test_unsupported_extension_errors() {
        let dir = tempdir().unwrap();
        let bad = dir.path().join("radr.txt");
        std::fs::write(&bad, "adr_dir=adrs").unwrap();
        let err = load_config(Some(&bad)).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Unsupported config extension"));
    }

    #[test]
    fn test_env_over_local_and_defaults() {
        let dir = tempdir().unwrap();
        // Write local toml
        let toml_path = dir.path().join("radr.toml");
        let mut f = std::fs::File::create(&toml_path).unwrap();
        writeln!(f, "adr_dir='local'\nindex_name='LOCAL.md'").unwrap();
        // Write env yaml
        let yaml_path = dir.path().join("radr.yaml");
        std::fs::write(&yaml_path, b"adr_dir: env\nindex_name: ENV.md\n").unwrap();
        // defaults before setting cwd/env
        let d = Config::default();
        assert_eq!(d.adr_dir, PathBuf::from("docs/adr"));
        assert_eq!(d.index_name, "index.md");
        // Now set cwd and env; env should win when no CLI provided
        std::env::set_current_dir(dir.path()).unwrap();
        std::env::set_var("RADR_CONFIG", yaml_path.to_str().unwrap());
        let cfg = load_config(None).unwrap();
        assert_eq!(cfg.adr_dir, PathBuf::from("env"));
        assert_eq!(cfg.index_name, "ENV.md");
        std::env::remove_var("RADR_CONFIG");
    }

    #[test]
    fn test_invalid_config_content_errors() {
        let dir = tempdir().unwrap();
        let bad_toml = dir.path().join("radr.toml");
        // invalid toml (missing equals)
        std::fs::write(&bad_toml, "adr_dir 'oops'").unwrap();
        let err = load_config(Some(&bad_toml)).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("Parsing TOML config"));
    }
}
