use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use ojg_core::cli::{Cli, Command};
use ojg_core::config::{load_config, ProcessEnv};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ojg: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let _config = load_config(&cli.overrides(), &cwd, &ProcessEnv)?;

    match cli.command {
        None => Err(anyhow::anyhow!("a search query is required")),
        Some(Command::Search(_)) => Err(anyhow::anyhow!("search is not available yet")),
        Some(Command::Init) => Err(anyhow::anyhow!("initialization is not available yet")),
        Some(Command::Doctor) => Err(anyhow::anyhow!("doctor is not available yet")),
        Some(Command::Inspect(_)) => Err(anyhow::anyhow!("inspection is not available yet")),
        Some(Command::Exact(_)) => Err(anyhow::anyhow!("exact search is not available yet")),
        Some(Command::Models) => Err(anyhow::anyhow!("model listing is not available yet")),
        Some(Command::Config) => Err(anyhow::anyhow!("config display is not available yet")),
        Some(Command::Cache(_)) => Err(anyhow::anyhow!("cache management is not available yet")),
        Some(Command::Mcp) => Err(anyhow::anyhow!("MCP is not available yet")),
        Some(Command::Completions(_)) => Err(anyhow::anyhow!("completions are not available yet")),
    }
}
