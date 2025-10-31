use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use radr::actions::{
    accept, create_new_adr, list_and_index, mark_superseded, reformat, reformat_all, reject,
};
use radr::config::load_config;
use radr::domain::parse_number;
use radr::repository::AdrRepository;
use radr::{Config, FsAdrRepository};

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
    },
    /// Create a new ADR that supersedes an existing ADR number
    Supersede {
        /// ADR number to supersede (e.g., 0003 or 3)
        id: String,
        /// Title for the new ADR
        title: String,
        /// Force superseding even if already superseded
        #[arg(long)]
        force: bool,
    },
    /// Accept an ADR by id or title
    Accept {
        /// ADR id (number) or exact title
        id_or_title: String,
    },
    /// Reject an ADR by id or title
    Reject {
        /// ADR id (number) or exact title
        id_or_title: String,
    },
    /// List ADRs found in the ADR directory
    List,
    /// Regenerate the index.md file
    Index,
    /// Reformat ADR(s) to the current config (format/front matter)
    #[command(
        about = "Reformat ADR(s) to the current config",
        long_about = "Converts ADR content and filename to match the current config (format/front matter). \
Use --all to reformat every ADR; otherwise pass a single ADR id. \
Cross-links in Supersedes lines and the index are updated accordingly.\n\nExamples:\n  radr reformat 3\n  radr reformat --all"
    )]
    Reformat {
        /// Reformat all ADRs to current config
        #[arg(long, help = "Reformat every ADR in the repository")]
        all: bool,
        /// ADR number to reformat (e.g., 0003 or 3). Ignored if --all is set.
        #[arg(help = "ADR number to reformat; omit with --all")]
        id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg: Config = load_config(cli.config.as_ref())?;

    fs::create_dir_all(&cfg.adr_dir)
        .with_context(|| format!("Creating ADR directory at {}", cfg.adr_dir.display()))?;

    let repo = FsAdrRepository::new(&cfg.adr_dir);

    match cli.command {
        Commands::New { title } => {
            let meta = create_new_adr(&repo, &cfg, &title, None)?;
            println!(
                "Created ADR {:04}: {} at {}",
                meta.number,
                meta.title,
                meta.path.display()
            );
        }
        Commands::Supersede { id, title, force } => {
            let old_num = parse_number(&id)?;
            // Pre-check: if target ADR is already superseded, print helpful message and exit with error
            if !force {
                if let Ok(existing) = repo.list() {
                    if let Some(old) = existing.iter().find(|a| a.number == old_num) {
                        if let Some(sb) = old.superseded_by {
                            let sb_title = existing
                                .iter()
                                .find(|a| a.number == sb)
                                .map(|a| a.title.as_str())
                                .unwrap_or("");
                            eprintln!(
                                "{:04}: {} is already superseded by {:04}: {}",
                                old.number, old.title, sb, sb_title
                            );
                            return Err(anyhow!("ADR already superseded"));
                        }
                    }
                }
            }

            let new_meta = create_new_adr(&repo, &cfg, &title, Some(old_num))?;
            mark_superseded(&repo, &cfg, old_num, new_meta.number)?;
            println!(
                "Created ADR {:04} superseding {:04}",
                new_meta.number, old_num
            );
        }
        Commands::Accept { id_or_title } => {
            let updated = accept(&repo, &cfg, &id_or_title)?;
            println!("Accepted ADR {:04}: {}", updated.number, updated.title);
        }
        Commands::Reject { id_or_title } => {
            let updated = reject(&repo, &cfg, &id_or_title)?;
            println!("Rejected ADR {:04}: {}", updated.number, updated.title);
        }
        Commands::List | Commands::Index => {
            let adrs = list_and_index(&repo, &cfg)?;
            for a in &adrs {
                println!("{:04} | {} | {} | {}", a.number, a.title, a.status, a.date);
            }
            println!("Updated {}", cfg.adr_dir.join(&cfg.index_name).display());
        }
        Commands::Reformat { all, id } => {
            if all {
                let updated = reformat_all(&repo, &cfg)?;
                println!(
                    "Reformatted {} ADR(s) to {} (front matter: {})",
                    updated.len(),
                    cfg.format,
                    cfg.front_matter
                );
            } else {
                let id =
                    id.ok_or_else(|| anyhow::anyhow!("Missing ADR id. Pass an id or use --all"))?;
                let n = parse_number(&id)?;
                let updated = reformat(&repo, &cfg, n)?;
                println!(
                    "Reformatted ADR {:04}: {} to {} (front matter: {})",
                    updated.number, updated.title, cfg.format, cfg.front_matter
                );
            }
        }
    }

    Ok(())
}
