use std::fs;
use std::path::PathBuf;

use anyhow::{Result, Context};
use clap::{Parser, Subcommand};

use radr::config::load_config;
use radr::domain::parse_number;
use radr::{Config, FsAdrRepository};
use radr::usecase::{create_new_adr, mark_superseded, list_and_index, accept};

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
    },
    /// Accept an ADR by id or title
    Accept {
        /// ADR id (number) or exact title
        id_or_title: String,
    },
    /// List ADRs found in the ADR directory
    List,
    /// Regenerate the index.md file
    Index,
}

// All core logic is now in the library modules.

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
        Commands::Supersede { id, title } => {
            let old_num = parse_number(&id)?;
            let new_meta = create_new_adr(&repo, &cfg, &title, Some(old_num))?;
            mark_superseded(&repo, &cfg, old_num, new_meta.number)?;
            println!(
                "Created ADR {:04} superseding {:04}",
                new_meta.number, old_num
            );
        }
        Commands::Accept { id_or_title } => {
            let updated = accept(&repo, &cfg, &id_or_title)?;
            println!(
                "Accepted ADR {:04}: {}",
                updated.number, updated.title
            );
        }
        Commands::List | Commands::Index => {
            let adrs = list_and_index(&repo, &cfg)?;
            for a in &adrs {
                println!(
                    "{:04} | {} | {} | {}",
                    a.number, a.title, a.status, a.date
                );
            }
            println!(
                "Updated {}",
                cfg.adr_dir.join(&cfg.index_name).display()
            );
        }
    }

    Ok(())
}


