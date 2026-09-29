use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ojg",
    version,
    about = "OpenJevGrep - local-first semantic grep powered by OpenJev"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    #[command(flatten)]
    pub search: SearchFlags,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Search(Box<SearchCommand>),
    Init,
    Doctor,
    Inspect(InspectCommand),
    Exact(ExactCommand),
    Models,
    Config,
    Cache(CacheCommand),
    Mcp,
    Completions(CompletionsCommand),
}

#[derive(Debug, Args)]
pub struct SearchCommand {
    #[arg(value_name = "QUERY")]
    pub query: String,

    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    #[command(flatten)]
    pub flags: SearchFlags,
}

#[derive(Debug, Args, Clone, Default)]
pub struct SearchFlags {
    #[arg(long)]
    pub threshold: Option<f64>,

    #[arg(long)]
    pub limit: Option<usize>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub endpoint: Option<String>,

    #[arg(long = "scope", value_name = "PATH", action = clap::ArgAction::Append)]
    pub scopes: Vec<PathBuf>,

    #[arg(long = "exclude", value_name = "PATTERN", action = clap::ArgAction::Append)]
    pub excludes: Vec<String>,

    #[arg(long = "include", value_name = "PATTERN", action = clap::ArgAction::Append)]
    pub includes: Vec<String>,

    #[arg(long)]
    pub max_file_size: Option<u64>,

    #[arg(long)]
    pub max_chunk_lines: Option<usize>,

    #[arg(long)]
    pub context: Option<usize>,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub compact: bool,

    #[arg(long)]
    pub files: bool,

    #[arg(long)]
    pub no_cache: bool,

    #[arg(long)]
    pub no_gitignore: bool,

    #[arg(long)]
    pub hidden: bool,

    #[arg(long)]
    pub follow: bool,

    #[arg(long)]
    pub debug: bool,
}

#[derive(Debug, Args)]
pub struct InspectCommand {
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ExactCommand {
    #[arg(value_name = "QUERY")]
    pub query: String,

    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CacheCommand {
    #[arg(long)]
    pub clear: bool,
}

#[derive(Debug, Args)]
pub struct CompletionsCommand {
    #[arg(value_name = "SHELL")]
    pub shell: String,
}

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub threshold: Option<f64>,
    pub limit: Option<usize>,
    pub batch_size: Option<usize>,
    pub concurrency: Option<usize>,
    pub max_file_size: Option<u64>,
    pub max_chunk_lines: Option<usize>,
    pub context_lines: Option<usize>,
    pub no_cache: bool,
}

impl Cli {
    pub fn overrides(&self) -> CliOverrides {
        self.search.overrides()
    }
}

impl SearchFlags {
    pub fn overrides(&self) -> CliOverrides {
        CliOverrides {
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            threshold: self.threshold,
            limit: self.limit,
            max_file_size: self.max_file_size,
            max_chunk_lines: self.max_chunk_lines,
            context_lines: self.context,
            no_cache: self.no_cache,
            ..CliOverrides::default()
        }
    }
}
