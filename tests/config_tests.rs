use std::collections::HashMap;
use std::fs;

use clap::Parser;
use ojg_core::cli::{Cli, CliOverrides};
use ojg_core::config::{load_config_from_paths, EnvSource};
use tempfile::tempdir;

struct TestEnv(HashMap<String, String>);

impl EnvSource for TestEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

#[test]
fn uses_documented_defaults_without_config_files() {
    let root = tempdir().expect("temporary root");
    let env = TestEnv(HashMap::new());

    let config = load_config_from_paths(&CliOverrides::default(), root.path(), &env, None)
        .expect("defaults should load");

    assert_eq!(config.backend.endpoint, "http://127.0.0.1:8080");
    assert_eq!(config.backend.model, "verdict-1.4");
    assert_eq!(config.search.threshold, 0.70);
    assert_eq!(config.search.limit, 20);
    assert_eq!(config.search.batch_size, 16);
    assert_eq!(config.search.concurrency, 4);
    assert_eq!(config.search.max_file_size, 1_048_576);
    assert_eq!(config.search.max_chunk_lines, 160);
    assert_eq!(config.search.context_lines, 2);
}

#[test]
fn applies_cli_then_environment_then_project_then_user_precedence() {
    let root = tempdir().expect("temporary root");
    let user = tempdir().expect("temporary user config");
    fs::write(
        user.path().join("config.toml"),
        "[backend]\nmodel = \"user-model\"\n[search]\nthreshold = 0.55\n",
    )
    .expect("user config");
    fs::write(
        root.path().join(".openjevgrep.toml"),
        "[backend]\nmodel = \"project-model\"\n[search]\nthreshold = 0.65\n",
    )
    .expect("project config");
    let mut vars = HashMap::new();
    vars.insert("OPENJEV_MODEL".to_string(), "env-model".to_string());
    vars.insert("OJG_THRESHOLD".to_string(), "0.75".to_string());
    let env = TestEnv(vars);
    let cli = CliOverrides {
        model: Some("cli-model".to_string()),
        ..CliOverrides::default()
    };

    let config = load_config_from_paths(
        &cli,
        root.path(),
        &env,
        Some(&user.path().join("config.toml")),
    )
    .expect("precedence should load");

    assert_eq!(config.backend.model, "cli-model");
    assert_eq!(config.search.threshold, 0.75);
}

#[test]
fn rejects_invalid_threshold() {
    let root = tempdir().expect("temporary root");
    fs::write(
        root.path().join(".openjevgrep.toml"),
        "[search]\nthreshold = 1.5\n",
    )
    .expect("project config");

    let error = load_config_from_paths(
        &CliOverrides::default(),
        root.path(),
        &TestEnv(HashMap::new()),
        None,
    )
    .expect_err("invalid threshold must fail");

    assert!(error.to_string().contains("threshold"));
}

#[test]
fn parses_shorthand_and_explicit_search_with_repeated_scopes() {
    let shorthand = Cli::try_parse_from([
        "ojg",
        "where is auth checked?",
        "src",
        "--scope",
        "tests",
        "--scope",
        "benches",
    ])
    .expect("shorthand");
    assert_eq!(shorthand.query.as_deref(), Some("where is auth checked?"));
    assert_eq!(
        shorthand.path.as_deref().map(|p| p.to_str().unwrap()),
        Some("src")
    );
    assert_eq!(shorthand.search.scopes.len(), 2);

    let explicit = Cli::try_parse_from(["ojg", "search", "where is auth checked?", "src"])
        .expect("explicit search");
    assert!(matches!(
        explicit.command,
        Some(ojg_core::cli::Command::Search(_))
    ));
}
