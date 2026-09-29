use std::process::ExitCode;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use ojg_core::backend::openjev::OpenJevBackend;
use ojg_core::backend::DecisionBackend;
use ojg_core::cache::{default_cache_directory, FileScoreCache, ScoreCache};
use ojg_core::cli::{Cli, Command, SearchFlags};
use ojg_core::commands::{check_backend, inspect_repository, InspectOptions};
use ojg_core::config::{load_config, AppConfig, ProcessEnv};
use ojg_core::exact::{exact_search, ExactOptions};
use ojg_core::mcp::run_stdio;
use ojg_core::output::{render_json, render_text, OutputMode};
use ojg_core::search::{search, search_with_cache, BackendMetadata, SearchOptions, SearchRequest};
use std::path::PathBuf;
use std::sync::Arc;

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
    let overrides = match &cli.command {
        Some(Command::Search(args)) => args.flags.overrides(),
        _ => cli.overrides(),
    };
    let config = load_config(&overrides, &cwd, &ProcessEnv)?;

    match cli.command {
        None => {
            let query = cli
                .query
                .ok_or_else(|| anyhow::anyhow!("a search query is required"))?;
            run_search(&config, query, cli.path, &cli.search, cwd)
        }
        Some(Command::Search(args)) => run_search(&config, args.query, args.path, &args.flags, cwd),
        Some(Command::Init) => {
            println!("OpenJevGrep initialized.");
            println!(
                "Repository: {}",
                ojg_core::config::repository_root(&cwd)?.display()
            );
            println!("Endpoint: {}", config.backend.endpoint);
            println!("Model: {}", config.backend.model);
            Ok(())
        }
        Some(Command::Doctor) => {
            let backend = OpenJevBackend::new(config.backend.clone(), reqwest::Client::new())?;
            let runtime = tokio::runtime::Runtime::new()?;
            let (health, selected, models) =
                runtime.block_on(check_backend(&backend, &config.backend.model))?;
            println!("repository: ok");
            println!(
                "OpenJev {}: {}",
                health.endpoint,
                if health.reachable {
                    "ok"
                } else {
                    "unavailable"
                }
            );
            println!(
                "model {}: {}",
                config.backend.model,
                if selected { "ok" } else { "missing" }
            );
            println!(
                "models: {}",
                models
                    .iter()
                    .map(|model| model.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Ok(())
        }
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
        Some(Command::Models) => {
            let backend = OpenJevBackend::new(config.backend.clone(), reqwest::Client::new())?;
            let runtime = tokio::runtime::Runtime::new()?;
            for model in runtime.block_on(backend.models())? {
                println!("{}", model.id);
            }
            Ok(())
        }
        Some(Command::Config) => {
            println!("endpoint = {}", config.backend.endpoint);
            println!("model = {}", config.backend.model);
            println!("threshold = {:.2}", config.search.threshold);
            Ok(())
        }
        Some(Command::Cache(args)) => {
            let cache = FileScoreCache::new(&default_cache_directory()?)?;
            if args.clear {
                let runtime = tokio::runtime::Runtime::new()?;
                runtime.block_on(cache.clear())?;
                println!("cache cleared");
            } else {
                println!("{}", default_cache_directory()?.display());
            }
            Ok(())
        }
        Some(Command::Mcp) => {
            let backend = Arc::new(OpenJevBackend::new(config.backend, reqwest::Client::new())?);
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(run_stdio(
                tokio::io::BufReader::new(tokio::io::stdin()),
                tokio::io::BufWriter::new(tokio::io::stdout()),
                backend,
            ))
        }
        Some(Command::Completions(args)) => {
            let shell = match args.shell.to_ascii_lowercase().as_str() {
                "bash" => Shell::Bash,
                "elvish" => Shell::Elvish,
                "fish" => Shell::Fish,
                "powershell" | "pwsh" => Shell::PowerShell,
                "zsh" => Shell::Zsh,
                value => return Err(anyhow::anyhow!("unsupported shell: {value}")),
            };
            let mut command = Cli::command();
            generate(shell, &mut command, "ojg", &mut std::io::stdout());
            Ok(())
        }
    }
}

fn run_search(
    config: &AppConfig,
    query: String,
    path: Option<PathBuf>,
    flags: &SearchFlags,
    cwd: PathBuf,
) -> Result<()> {
    let root = path.unwrap_or(cwd);
    let mut options = SearchOptions {
        threshold: config.search.threshold,
        limit: config.search.limit,
        batch_size: config.search.batch_size,
        concurrency: config.search.concurrency,
        max_file_size: config.search.max_file_size,
        max_chunk_lines: config.search.max_chunk_lines,
        context_lines: config.search.context_lines,
        hierarchical_threshold: config.search.hierarchical_threshold,
        scan: ojg_core::scanner::ScanOptions {
            excludes: flags.excludes.clone(),
            includes: flags.includes.clone(),
            no_gitignore: flags.no_gitignore,
            hidden: flags.hidden,
            follow: flags.follow,
            ..ojg_core::scanner::ScanOptions::default()
        },
    };
    if let Some(value) = flags.threshold {
        options.threshold = value;
    }
    if let Some(value) = flags.limit {
        options.limit = value;
    }
    if let Some(value) = flags.max_file_size {
        options.max_file_size = value;
    }
    if let Some(value) = flags.max_chunk_lines {
        options.max_chunk_lines = value;
    }
    if let Some(value) = flags.context {
        options.context_lines = value;
    }
    let backend = Arc::new(OpenJevBackend::new(
        config.backend.clone(),
        reqwest::Client::new(),
    )?);
    let request = SearchRequest {
        query,
        root,
        scopes: flags.scopes.clone(),
        backend: BackendMetadata {
            endpoint: config.backend.endpoint.clone(),
            model: config.backend.model.clone(),
        },
        options,
    };
    let runtime = tokio::runtime::Runtime::new()?;
    let response = if config.cache.enabled && !flags.no_cache {
        let cache = Arc::new(FileScoreCache::new(&default_cache_directory()?)?);
        runtime.block_on(search_with_cache(request, backend, cache))?
    } else {
        runtime.block_on(search(request, backend))?
    };
    if flags.json {
        println!("{}", render_json(&response)?);
    } else {
        let mode = if flags.files {
            OutputMode::Files
        } else if flags.compact {
            OutputMode::Compact
        } else {
            OutputMode::Human
        };
        print!("{}", render_text(&response, mode));
        eprintln!(
            "ojg: {} files, {} chunks, {} evaluated, {} matches, {} ms",
            response.coverage.files_scanned,
            response.coverage.chunks_found,
            response.coverage.chunks_evaluated,
            response.results.len(),
            response.timing.total_ms
        );
    }
    Ok(())
}
