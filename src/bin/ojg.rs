use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use ojg_core::cli::{Cli, Command};
use ojg_core::commands::{inspect_repository, InspectOptions};
use ojg_core::config::{load_config, ProcessEnv};
use ojg_core::exact::{exact_search, ExactOptions};

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
        Some(Command::Inspect(args)) => {
            let root = args.path.unwrap_or(cwd);
            let report = inspect_repository(&root, &InspectOptions::default())?;
            println!("files: {}", report.files_included);
            println!("chunks: {}", report.chunks_found);
            println!("estimated requests: {}", report.estimated_requests);
            println!("estimated input bytes: {}", report.estimated_input_bytes);
            Ok(())
        }
        Some(Command::Exact(args)) => {
            let root = args.path.unwrap_or(cwd);
            for item in exact_search(&args.query, &[root], &ExactOptions::default())? {
                println!("{}:{}:{}", item.path, item.line, item.text);
            }
            Ok(())
        }
        Some(Command::Models) => Err(anyhow::anyhow!("model listing is not available yet")),
        Some(Command::Config) => {
            println!("endpoint = {}", _config.backend.endpoint);
            println!("model = {}", _config.backend.model);
            println!("threshold = {:.2}", _config.search.threshold);
            Ok(())
        }
        Some(Command::Cache(_)) => Err(anyhow::anyhow!("cache management is not available yet")),
        Some(Command::Mcp) => Err(anyhow::anyhow!("MCP is not available yet")),
        Some(Command::Completions(_)) => Err(anyhow::anyhow!("completions are not available yet")),
    }
}
