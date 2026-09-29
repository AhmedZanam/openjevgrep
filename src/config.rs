use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::CliOverrides;
use crate::Result;

pub trait EnvSource {
    fn get(&self, key: &str) -> Option<String>;
}

pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub backend: BackendConfig,
    pub search: SearchConfig,
    pub cache: CacheConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackendConfig {
    pub endpoint: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub concurrency: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchConfig {
    pub threshold: f64,
    pub limit: usize,
    pub batch_size: usize,
    pub concurrency: usize,
    pub max_file_size: u64,
    pub max_chunk_lines: usize,
    pub context_lines: usize,
    pub hierarchical_threshold: usize,
    pub exclude: Vec<String>,
    pub include: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CacheConfig {
    pub enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            backend: BackendConfig {
                endpoint: "http://127.0.0.1:8080".to_string(),
                model: "verdict-1.4".to_string(),
                timeout_seconds: 30,
                concurrency: 4,
            },
            search: SearchConfig {
                threshold: 0.70,
                limit: 20,
                batch_size: 16,
                concurrency: 4,
                max_file_size: 1_048_576,
                max_chunk_lines: 160,
                context_lines: 2,
                hierarchical_threshold: 150,
                exclude: Vec::new(),
                include: Vec::new(),
            },
            cache: CacheConfig { enabled: true },
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    backend: FileBackendConfig,
    #[serde(default)]
    search: FileSearchConfig,
    #[serde(default)]
    cache: FileCacheConfig,
}

#[derive(Debug, Default, Deserialize)]
struct FileBackendConfig {
    endpoint: Option<String>,
    model: Option<String>,
    timeout_seconds: Option<u64>,
    concurrency: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct FileSearchConfig {
    threshold: Option<f64>,
    limit: Option<usize>,
    batch_size: Option<usize>,
    concurrency: Option<usize>,
    max_file_size: Option<u64>,
    max_chunk_lines: Option<usize>,
    context_lines: Option<usize>,
    hierarchical_threshold: Option<usize>,
    exclude: Option<Vec<String>>,
    include: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct FileCacheConfig {
    enabled: Option<bool>,
}

pub fn load_config(cli: &CliOverrides, cwd: &Path, env: &impl EnvSource) -> Result<AppConfig> {
    let user_path = dirs::config_dir().map(|path| path.join("openjevgrep").join("config.toml"));
    load_config_from_paths(cli, cwd, env, user_path.as_deref())
}

pub fn load_config_from_paths(
    cli: &CliOverrides,
    cwd: &Path,
    env: &impl EnvSource,
    user_path: Option<&Path>,
) -> Result<AppConfig> {
    let mut config = AppConfig::default();
    if let Some(path) = user_path {
        apply_file(&mut config, path)?;
    }
    apply_file(&mut config, &cwd.join(".openjevgrep.toml"))?;
    apply_environment(&mut config, env)?;
    apply_cli(&mut config, cli);
    validate(&config)?;
    Ok(config)
}

pub fn repository_root(path: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    let start = if canonical.is_file() {
        canonical
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("path has no parent: {}", canonical.display()))?
    } else {
        canonical
    };

    for ancestor in start.ancestors() {
        if ancestor.join(".git").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(start)
}

fn apply_file(config: &mut AppConfig, path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(path)?;
    let file: FileConfig = toml::from_str(&text)?;
    apply_file_config(config, file);
    Ok(())
}

fn apply_file_config(config: &mut AppConfig, file: FileConfig) {
    if let Some(value) = file.backend.endpoint {
        config.backend.endpoint = value;
    }
    if let Some(value) = file.backend.model {
        config.backend.model = value;
    }
    if let Some(value) = file.backend.timeout_seconds {
        config.backend.timeout_seconds = value;
    }
    if let Some(value) = file.backend.concurrency {
        config.backend.concurrency = value;
    }
    if let Some(value) = file.search.threshold {
        config.search.threshold = value;
    }
    if let Some(value) = file.search.limit {
        config.search.limit = value;
    }
    if let Some(value) = file.search.batch_size {
        config.search.batch_size = value;
    }
    if let Some(value) = file.search.concurrency {
        config.search.concurrency = value;
    }
    if let Some(value) = file.search.max_file_size {
        config.search.max_file_size = value;
    }
    if let Some(value) = file.search.max_chunk_lines {
        config.search.max_chunk_lines = value;
    }
    if let Some(value) = file.search.context_lines {
        config.search.context_lines = value;
    }
    if let Some(value) = file.search.hierarchical_threshold {
        config.search.hierarchical_threshold = value;
    }
    if let Some(value) = file.search.exclude {
        config.search.exclude = value;
    }
    if let Some(value) = file.search.include {
        config.search.include = value;
    }
    if let Some(value) = file.cache.enabled {
        config.cache.enabled = value;
    }
}

fn apply_environment(config: &mut AppConfig, env: &impl EnvSource) -> Result<()> {
    if let Some(value) = env.get("OPENJEV_URL") {
        config.backend.endpoint = value;
    }
    if let Some(value) = env.get("OPENJEV_MODEL") {
        config.backend.model = value;
    }
    if let Some(value) = env.get("OPENJEV_TIMEOUT_SECONDS") {
        config.backend.timeout_seconds = value.parse()?;
    }
    if let Some(value) = env.get("OJG_THRESHOLD") {
        config.search.threshold = value.parse()?;
    }
    if let Some(value) = env.get("OJG_LIMIT") {
        config.search.limit = value.parse()?;
    }
    if let Some(value) = env.get("OJG_BATCH_SIZE") {
        config.search.batch_size = value.parse()?;
    }
    if let Some(value) = env.get("OJG_CONCURRENCY") {
        config.search.concurrency = value.parse()?;
    }
    if let Some(value) = env.get("OJG_MAX_FILE_SIZE") {
        config.search.max_file_size = value.parse()?;
    }
    if let Some(value) = env.get("OJG_MAX_CHUNK_LINES") {
        config.search.max_chunk_lines = value.parse()?;
    }
    if let Some(value) = env.get("OJG_CONTEXT_LINES") {
        config.search.context_lines = value.parse()?;
    }
    if let Some(value) = env.get("OJG_NO_CACHE") {
        config.cache.enabled = !parse_bool(&value)?;
    }
    Ok(())
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow::anyhow!("invalid boolean value: {value}")),
    }
}

fn apply_cli(config: &mut AppConfig, cli: &CliOverrides) {
    if let Some(value) = &cli.endpoint {
        config.backend.endpoint = value.clone();
    }
    if let Some(value) = &cli.model {
        config.backend.model = value.clone();
    }
    if let Some(value) = cli.threshold {
        config.search.threshold = value;
    }
    if let Some(value) = cli.limit {
        config.search.limit = value;
    }
    if let Some(value) = cli.batch_size {
        config.search.batch_size = value;
    }
    if let Some(value) = cli.concurrency {
        config.search.concurrency = value;
    }
    if let Some(value) = cli.max_file_size {
        config.search.max_file_size = value;
    }
    if let Some(value) = cli.max_chunk_lines {
        config.search.max_chunk_lines = value;
    }
    if let Some(value) = cli.context_lines {
        config.search.context_lines = value;
    }
    if cli.no_cache {
        config.cache.enabled = false;
    }
}

fn validate(config: &AppConfig) -> Result<()> {
    if config.backend.endpoint.trim().is_empty() {
        return Err(anyhow::anyhow!("endpoint must not be empty"));
    }
    if config.backend.model.trim().is_empty() {
        return Err(anyhow::anyhow!("model must not be empty"));
    }
    if !(0.0..=1.0).contains(&config.search.threshold) {
        return Err(anyhow::anyhow!("threshold must be between 0.0 and 1.0"));
    }
    if config.search.limit == 0 {
        return Err(anyhow::anyhow!("limit must be greater than zero"));
    }
    if config.search.batch_size == 0 {
        return Err(anyhow::anyhow!("batch_size must be greater than zero"));
    }
    if config.search.concurrency == 0 || config.backend.concurrency == 0 {
        return Err(anyhow::anyhow!("concurrency must be greater than zero"));
    }
    if config.search.max_file_size == 0 || config.search.max_chunk_lines == 0 {
        return Err(anyhow::anyhow!(
            "file and chunk limits must be greater than zero"
        ));
    }
    Ok(())
}
