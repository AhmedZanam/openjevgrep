# OpenJevGrep

OpenJevGrep (`ojg`) is a local-first semantic search CLI for source repositories. It scans a repository safely, splits supported languages into source-aware chunks, sends relevance decisions to a local OpenJev service, and prints ranked results with source context.

## Supported systems

- Linux: x86_64 and aarch64
- macOS: Intel and Apple Silicon
- Windows: x86_64 and aarch64

The code uses Rust's cross-platform filesystem, networking, and path APIs. CI verifies Linux, macOS, and Windows builds on every push and pull request.

## Requirements

- Rust stable for building from source
- OpenJev running locally for semantic search

The default backend is:

```text
http://127.0.0.1:8080
```

Exact search, repository inspection, and cache management work without a running backend.

## Install

From a published release, download the archive for your operating system and CPU architecture, extract `ojg`, and place it on `PATH`.

From a checkout on any supported system:

```bash
cargo install --path .
```

To build a release binary without installing it:

```bash
cargo build --release
```

The binary is written to `target/release/ojg` on Unix-like systems and `target/release/ojg.exe` on Windows.

## Quick start

Start OpenJev, then search the current repository:

```bash
ojg "where is authentication checked?"
```

Search an explicit path:

```bash
ojg search "where are retries configured?" ./src
```

Useful output modes:

```bash
ojg "cache invalidation" --compact
ojg "cache invalidation" --files
ojg "cache invalidation" --json
```

Exact literal search does not contact OpenJev:

```bash
ojg exact "validate_access_token" .
```

Inspect scanning and chunk estimates:

```bash
ojg inspect .
```

## Search controls

```text
--threshold <FLOAT>       Minimum relevance probability, default 0.70
--limit <N>               Maximum results, default 20
--scope <PATH>            Restrict search; repeatable
--include <PATTERN>       Include a glob pattern; repeatable
--exclude <PATTERN>       Exclude a glob pattern; repeatable
--hidden                  Include hidden files
--follow                  Follow symlinks
--no-gitignore            Ignore .gitignore, .ignore, and .ojgignore
--no-cache                Disable score caching
--endpoint <URL>          OpenJev endpoint
--model <NAME>            OpenJev model
```

By default, generated files, common dependency/build directories, binary files, invalid UTF-8, oversized files, and sensitive files are excluded. Project ignore files are respected.

## Configuration

Configuration is loaded in this order, with later values taking precedence:

1. User config: the platform config directory followed by `openjevgrep/config.toml`
2. Project config: `.openjevgrep.toml`
3. Environment variables
4. Command-line flags

Example `.openjevgrep.toml`:

```toml
[backend]
endpoint = "http://127.0.0.1:8080"
model = "verdict-1.4"
timeout_seconds = 30

[search]
threshold = 0.70
limit = 20
batch_size = 16
concurrency = 4
max_file_size = 1048576
max_chunk_lines = 160
context_lines = 2
hierarchical_threshold = 150

[cache]
enabled = true
```

Typical user config locations are `$XDG_CONFIG_HOME/openjevgrep/config.toml` on Linux, `~/Library/Application Support/openjevgrep/config.toml` on macOS, and `%APPDATA%\\openjevgrep\\config.toml` on Windows.

Environment variables include `OPENJEV_URL`, `OPENJEV_MODEL`, `OPENJEV_TIMEOUT_SECONDS`, `OJG_THRESHOLD`, `OJG_LIMIT`, `OJG_BATCH_SIZE`, `OJG_CONCURRENCY`, `OJG_MAX_FILE_SIZE`, `OJG_MAX_CHUNK_LINES`, `OJG_CONTEXT_LINES`, and `OJG_NO_CACHE`.

Use `ojg config` to display the active endpoint, model, threshold, and limit.

## Shell completions

Generate completions for Bash, Fish, PowerShell, Zsh, or Elvish:

```bash
ojg completions bash > ojg.bash
ojg completions fish > ojg.fish
ojg completions powershell > ojg.ps1
ojg completions zsh > _ojg
```

## MCP

Run the newline-delimited JSON-RPC MCP server over standard input/output:

```bash
ojg mcp
```

The server exposes `semantic_search_code`. Diagnostics stay off standard output so the protocol stream remains valid.

## Evaluation

Evaluate retrieval against annotated questions:

```bash
cargo run --release --bin ojg-eval -- \
  --root fixtures/sample-repo \
  --questions eval/questions.json
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

The OpenJev service is intentionally separate from this repository. Backend tests use a local HTTP test server; live backend checks require an OpenJev instance at the configured endpoint.
