# OpenJevGrep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and verify a production-quality Rust `ojg` CLI that performs local-first semantic source search through an OpenJev-compatible System One backend, with safe scanning, exact locations, JSON/MCP interfaces, caching, evaluation, and release documentation.

**Architecture:** Use one Cargo package with a reusable `ojg_core` library and `ojg`/`ojg-eval` binaries. Keep configuration, scanning, chunking, backend, search, cache, output, exact search, and MCP as focused modules. Drive the implementation from deterministic fixtures and a fake HTTP backend; live OpenJev is opt-in.

**Tech Stack:** Stable Rust; `clap`, `tokio`, `reqwest`, `serde`, `serde_json`, `ignore`, `walkdir`, `globset`, `toml`, `dirs`, `sha2`, `thiserror`, `anyhow`, `tracing`, `tracing-subscriber`, `tree-sitter` plus the seven requested grammars, `criterion` for benchmarks, and `assert_cmd`/`tempfile`/`wiremock` or an equivalent local test server for integration tests.

**Spec:** `docs/superpowers/specs/2026-09-28-openjevgrep-design.md`

## Global Constraints

- The executable is `ojg`; the package/library names must not expose personal names or agent names.
- Default endpoint is `http://127.0.0.1:8080`; default model is `verdict-1.4`; arbitrary model strings remain valid.
- Public probabilities are only OpenJev-returned `answers[*].noul` values, clamped/rejected only when the API response violates `0.0..=1.0`.
- No mandatory embeddings, vector database, persistent repository index, daemon, cloud provider, Node.js runtime, or Python runtime dependency for `ojg`.
- Source displayed in results is reread from the repository; generated text is never displayed as source.
- Standard ignore files and `.ojgignore` are respected; binary, invalid UTF-8, secret-pattern, and oversized files are excluded with reasons.
- Default symlink behavior is no-follow; diagnostics use stderr and JSON results use stdout.
- CLI > environment > project `.openjevgrep.toml` > user config > built-in defaults.
- Every task follows RED -> GREEN -> focused verification -> conventional commit.

## Review Focus

- A repository containing malformed UTF-8, binary files, symlink cycles, and a file exactly at the size limit must scan deterministically without panic or traversal escape. Pin in Task 2 with `scanner_handles_binary_invalid_utf8_and_symlinks`.
- An OpenJev response with missing question ids, non-numeric `noul`, or a probability outside `[0,1]` must be rejected as a backend error and must not become a heuristic score. Pin in Task 3 with `rejects_malformed_probability_answers`.
- A candidate that spans the model context limit must be split or truncated with preserved line coverage, never silently omitted. Pin in Task 5 with `oversized_chunk_preserves_source_lines`.
- A partially failing batch must return successful matches plus `coverage.partial = true` and a reason, not claim absence. Pin in Task 4 with `partial_backend_failure_sets_coverage`.
- JSON output must remain parseable when debug/progress logging is enabled, and MCP stdout must contain only JSON-RPC. Pin in Task 7 with `json_output_is_clean_stdout` and `mcp_keeps_diagnostics_off_stdout`.

## File Map

Initial source layout:

```text
Cargo.toml
src/lib.rs
src/error.rs
src/config.rs
src/scanner.rs
src/chunk.rs
src/backend/mod.rs
src/backend/openjev.rs
src/cache.rs
src/search.rs
src/output.rs
src/exact.rs
src/mcp.rs
src/bin/ojg.rs
src/bin/ojg_eval.rs
tests/cli_integration.rs
tests/fake_backend.rs
fixtures/sample-repo/...
eval/questions.json
benches/search.rs
docs/...
.github/...
```

The plan may split a module into focused submodules when it exceeds one responsibility, but later tasks must preserve the interfaces listed below.

### Task 1: Package bootstrap, configuration, and command model

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `src/lib.rs`, `src/error.rs`, `src/config.rs`, `src/bin/ojg.rs`
- Test: `tests/config_tests.rs`, `tests/cli_help.rs`

**Interfaces:**
- Produces `pub struct AppConfig`, `BackendConfig`, `SearchConfig`, `CacheConfig` and `pub fn load_config(cli: &CliOverrides, cwd: &Path, env: impl EnvSource) -> Result<AppConfig>`.
- Produces `pub struct Cli` with shorthand query parsing, explicit `search`, repeated `--scope`, and all search flags from the specification.
- Produces `pub fn repository_root(path: &Path) -> Result<PathBuf>` and stable clap command names for later handlers.

- [ ] **Step 1: Write failing config tests** for built-in defaults, CLI-over-environment precedence, environment-over-project precedence, invalid threshold, repeated scopes, and shorthand search parsing.
- [ ] **Step 2: Run `cargo test --test config_tests --test cli_help` and verify the new package/types are absent or the assertions fail for the intended reason.
- [ ] **Step 3: Implement package metadata, error type, clap command model, config structs, TOML loading, environment mapping, validation, and repository-root discovery. Keep command handlers returning a typed `Command` enum or an explicit unimplemented error until their tasks land.
- [ ] **Step 4: Run the focused tests and `cargo run -- --help`; verify defaults include endpoint, `verdict-1.4`, threshold `0.70`, limit `20`, batch size `16`, concurrency `4`, max file size `1048576`, max chunk lines `160`, and context lines `2`.
- [ ] **Step 5: Commit `feat: bootstrap ojg configuration and cli model`.

### Task 2: Safe repository scanning and deterministic fallback chunking

**Files:**
- Create: `src/scanner.rs`, `src/chunk.rs`, `tests/scanner_tests.rs`, `tests/chunk_tests.rs`
- Modify: `src/lib.rs`, `src/config.rs`

**Interfaces:**
- Produces `pub struct ScannedFile { pub path: PathBuf, pub relative_path: PathBuf, pub content: String, pub language: Option<String> }` and `pub struct ScanReport { pub included: Vec<ScannedFile>, pub excluded: Vec<Exclusion> }`.
- Produces `pub fn scan_repository(root: &Path, options: &ScanOptions) -> Result<ScanReport>`.
- Produces `pub struct SourceChunk { path: PathBuf, language: Option<String>, start_line: usize, end_line: usize, symbol: Option<String>, kind: ChunkKind, content: String }` and `pub fn fallback_chunks(file: &ScannedFile, max_lines: usize, context_lines: usize) -> Vec<SourceChunk>`.

- [ ] **Step 1: Write failing scanner tests** for `.gitignore`, `.ignore`, `.ojgignore`, default dependency/build directories, hidden-file behavior, secret exclusions, binary/invalid UTF-8 detection, maximum file size, and no-follow symlink behavior. Include the review-focus fixture with a symlink cycle.
- [ ] **Step 2: Run the scanner tests and verify each new behavior fails before implementation.**
- [ ] **Step 3: Implement scanner discovery with `ignore`/`walkdir`, normalized repository-relative paths, default excludes, glob includes/excludes, secret-file rules, UTF-8 validation, and exclusion reasons. Preserve explicit include overrides for safe directory defaults while keeping secret protection.
- [ ] **Step 4: Write failing fallback chunk tests** for paragraph blocks, overlapping line windows, exact 1-based line numbers, symbol-less text, and oversized content coverage.
- [ ] **Step 5: Implement deterministic fallback chunking with no fabricated lines and stable ordering.
- [ ] **Step 6: Run both test files, `cargo fmt --check`, and a temporary-directory scan smoke test; verify the review-focus test passes.
- [ ] **Step 7: Commit `feat: add safe repository scanning and fallback chunks`.

### Task 3: OpenJev backend contract and fake-server integration

**Files:**
- Create: `src/backend/mod.rs`, `src/backend/openjev.rs`, `tests/backend_tests.rs`, `tests/fake_backend.rs`
- Modify: `src/error.rs`, `src/config.rs`, `src/lib.rs`

**Interfaces:**
- Produces `pub struct Candidate { pub id: String, pub path: PathBuf, pub symbol: Option<String>, pub start_line: usize, pub end_line: usize, pub content: String }`.
- Produces `pub struct Score { pub candidate_id: String, pub probability: f64 }`, `BackendHealth`, `ModelInfo`, and `#[async_trait] pub trait DecisionBackend: Send + Sync { async fn health(&self) -> Result<BackendHealth>; async fn models(&self) -> Result<Vec<ModelInfo>>; async fn score_batch(&self, query: &str, candidates: &[Candidate]) -> Result<Vec<Score>>; }`.
- Produces `OpenJevBackend::new(config: BackendConfig, client: reqwest::Client) -> Result<Self>` and strict serializers for `/v1/systemone` and `/v1/models`.

- [ ] **Step 1: Write failing request/response tests** asserting the exact endpoint, model, state delimiters, candidate question ids, `noul` type/criteria, model parsing, timeout/retry classification, and malformed answer rejection.
- [ ] **Step 2: Run focused backend tests and verify request serialization and parser tests fail before implementation.
- [ ] **Step 3: Implement request DTOs, OpenJev HTTP calls, bounded retry for 429/529/5xx, timeout handling, model discovery, and strict `answers[*].noul` parsing. Reject missing ids, non-numeric values, and values outside `[0,1]`.
- [ ] **Step 4: Implement the fake backend server used by integration tests, including deterministic score maps, model listing, delay, and one-shot failure responses.
- [ ] **Step 5: Run `cargo test --test backend_tests --test fake_backend`; verify the review-focus malformed-probability test passes and no test contacts the network.
- [ ] **Step 6: Commit `feat: integrate OpenJev decision backend`.

### Task 4: Search pipeline, ranking, merging, coverage, and output

**Files:**
- Create: `src/search.rs`, `src/output.rs`, `tests/search_tests.rs`, `tests/output_tests.rs`
- Modify: `src/lib.rs`, `src/config.rs`, `src/backend/mod.rs`

**Interfaces:**
- Produces `pub struct SearchRequest { pub query: String, pub root: PathBuf, pub scopes: Vec<PathBuf>, pub options: SearchOptions }`, `SearchOptions`, `SearchResult`, `Coverage`, `Timing`, and `SearchResponse` matching the documented JSON contract.
- Produces `pub async fn search(request: SearchRequest, backend: Arc<dyn DecisionBackend>, cache: Option<Arc<dyn ScoreCache>>) -> Result<SearchResponse>`.
- Produces `pub fn render_text(response: &SearchResponse, mode: OutputMode) -> String` and `pub fn render_json(response: &SearchResponse) -> Result<String>`.

- [ ] **Step 1: Write failing search tests** for threshold `0.70`, limit, probability/path/line tie-breaking, adjacent/overlapping merge, duplicate suppression, and source reread. Add `partial_backend_failure_sets_coverage` with successful and failed fake batches.
- [ ] **Step 2: Run focused search tests and verify failures.**
- [ ] **Step 3: Implement direct chunk preparation, bounded batch dispatch through the backend trait, deterministic sort, threshold/limit, merge, coverage, and timing. A failed batch contributes a partial reason and does not create false negative claims.
- [ ] **Step 4: Write failing output tests** for human, compact, files-only, and versioned JSON output, including original source and exact line ranges.
- [ ] **Step 5: Implement output formatters, TTY/`NO_COLOR` behavior, stderr statistics, and JSON-safe serialization. Add `json_output_is_clean_stdout` using the CLI process.
- [ ] **Step 6: Wire the shorthand and `search` command to run a real scan/chunk/search/output path with the fake backend available to integration tests.
- [ ] **Step 7: Run `cargo test --test search_tests --test output_tests --test fake_backend` and commit `feat: implement semantic search and result output`.

### Task 5: Tree-sitter chunking, size control, and hierarchical retrieval

**Files:**
- Create: `src/chunk/language.rs`, `src/chunk/tree_sitter.rs`, `src/retrieval.rs`, `tests/tree_sitter_tests.rs`, `tests/retrieval_tests.rs`
- Modify: `src/chunk.rs`, `src/search.rs`, `Cargo.toml`

**Interfaces:**
- Produces `pub fn language_for_path(path: &Path) -> Option<LanguageKind>` for Rust, Python, JavaScript, TypeScript, TSX, Java, and Go.
- Produces `pub fn source_chunks(file: &ScannedFile, options: &ChunkOptions) -> Result<Vec<SourceChunk>>` with functions/methods/classes/structs/impls/traits/interfaces/modules/top-level declarations where the grammar exposes them.
- Produces `pub fn deterministic_file_preview(file: &ScannedFile, chunks: &[SourceChunk], max_chars: usize) -> String` and `pub fn split_for_backend(chunk: &SourceChunk, max_lines: usize, max_chars: usize) -> Vec<SourceChunk>`.

- [ ] **Step 1: Write failing Tree-sitter tests** for one representative function/class/declaration in each supported language, symbols, kind, and exact 1-based ranges.
- [ ] **Step 2: Run the grammar tests and verify failures before adapters exist.
- [ ] **Step 3: Implement isolated grammar adapters and fallback to deterministic chunks on parse errors or unsupported extensions.
- [ ] **Step 4: Write failing size-control tests** for an oversized declaration and context preservation; verify every output line maps to the original source.
- [ ] **Step 5: Implement declaration splitting and line-window fallback with coverage metadata; never drop an oversized candidate silently.
- [ ] **Step 6: Write failing hierarchical tests** for the `<150` direct path, `>=150` preview-pruning path, conservative file selection, and exact preview contents.
- [ ] **Step 7: Implement file-preview scoring through the same backend abstraction, then chunk scoring for qualifying files; expose configured chunk threshold and partial coverage.
- [ ] **Step 8: Run all chunk/retrieval/search tests, including `oversized_chunk_preserves_source_lines`, and commit `feat: add source-aware hierarchical retrieval`.

### Task 6: Cache, diagnostics, exact search, and workflow subcommands

**Files:**
- Create: `src/cache.rs`, `src/exact.rs`, `src/commands.rs`, `tests/cache_tests.rs`, `tests/exact_tests.rs`, `tests/commands_tests.rs`
- Modify: `src/bin/ojg.rs`, `src/config.rs`, `src/search.rs`, `src/output.rs`

**Interfaces:**
- Produces `#[async_trait] pub trait ScoreCache { async fn get(&self, key: &CacheKey) -> Result<Option<Score>>; async fn put(&self, key: &CacheKey, score: &Score) -> Result<()>; async fn clear(&self) -> Result<()>; }` and a filesystem implementation.
- Produces `pub fn exact_search(query: &str, roots: &[PathBuf], options: &ExactOptions) -> Result<Vec<ExactMatch>>` using `rg` when available and a safe built-in fallback.
- Produces command handlers for `init`, `doctor`, `inspect`, `models`, `config`, and `cache`; `inspect` returns included/excluded reasons, chunks, estimated requests, and input-size estimate.

- [ ] **Step 1: Write failing cache tests** for hit, miss, invalidation by content/query/model/endpoint, corrupt entry recovery, and atomic write behavior.
- [ ] **Step 2: Run cache tests and verify failure.**
- [ ] **Step 3: Implement the content-addressed filesystem cache with platform cache directory resolution and `--no-cache` handling.
- [ ] **Step 4: Write failing exact-search tests** for `rg` and fallback behavior, path scopes, literal matching, binary exclusion, and stable line numbers.
- [ ] **Step 5: Implement exact search with process exit handling and built-in fallback.
- [ ] **Step 6: Write failing command tests** for `doctor` without a backend, `models`, `inspect`, non-interactive `init`, and cache clearing; output must give actionable status without installing software.
- [ ] **Step 7: Implement command handlers and connect them to the CLI, preserving stderr/stdout separation.
- [ ] **Step 8: Run cache/exact/command tests plus CLI smoke tests and commit `feat: complete local workflow commands and cache`.

### Task 7: MCP stdio, evaluation suite, benchmarks, and integration coverage

**Files:**
- Create: `src/mcp.rs`, `src/bin/ojg_eval.rs`, `tests/cli_integration.rs`, `tests/mcp_tests.rs`, `fixtures/sample-repo/**`, `eval/questions.json`, `benches/search.rs`
- Modify: `src/bin/ojg.rs`, `src/search.rs`, `Cargo.toml`

**Interfaces:**
- Produces MCP tool `semantic_search_code` with arguments `{ query, path?, scopes?, threshold?, limit?, model?, endpoint? }` and structured JSON-RPC errors/results.
- Produces evaluation output with Recall@1, Recall@5, Recall@10, and MRR from annotated expected files.
- Produces a criterion benchmark group for scan, chunk, cache, fake backend evaluation, and full search.

- [ ] **Step 1: Write failing MCP tests** for valid tool calls, invalid arguments, backend errors, newline-delimited requests, and `mcp_keeps_diagnostics_off_stdout`.
- [ ] **Step 2: Run MCP tests and verify failures.**
- [ ] **Step 3: Implement the stdio JSON-RPC loop, tool schema/validation, structured response mapping, cancellation on EOF, and stderr-only diagnostics.
- [ ] **Step 4: Write failing CLI integration tests** covering shorthand search, explicit `search`, JSON, compact, files-only, exact, partial backend errors, and the fixture repository.
- [ ] **Step 5: Implement fixture wiring and any missing command integration; assert exact source/line ranges and ranked fake probabilities.
- [ ] **Step 6: Write evaluation tests for Recall@K/MRR and create representative fixture behaviors and annotated questions.
- [ ] **Step 7: Implement `ojg-eval` and benchmark groups without live network calls.
- [ ] **Step 8: Run `cargo test --all`, `cargo bench --no-run`, and the review-focus JSON/MCP tests; commit `feat: add MCP and retrieval evaluation tooling`.

### Task 8: Documentation, licensing, CI, release metadata, and final verification

**Files:**
- Create/modify: `README.md`, `LICENSE`, `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, `docs/architecture.md`, `docs/configuration.md`, `docs/openjev.md`, `docs/mcp.md`, `docs/security.md`, `docs/benchmarks.md`, `AGENTS.md`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/workflows/security.yml`, `.github/ISSUE_TEMPLATE/**`, `.github/dependabot.yml`, `.gitignore`

**Interfaces:**
- Documentation commands must match the built CLI and use current OpenJev setup instructions verified against the upstream project.
- CI runs fmt, clippy with `-D warnings`, all tests, and release build on Ubuntu, Windows, and macOS without live model downloads.
- Release workflow builds the requested practical targets, archives obvious `ojg-v<version>-<target>` names, and emits `SHA256SUMS`.

- [ ] **Step 1: Write documentation smoke checks** that extract/readme command examples and assert the CLI accepts their flags; add a security-doc checklist for endpoint, cache, ignores, symlinks, secret exclusions, and no telemetry.
- [ ] **Step 2: Run the documentation checks and record any mismatch.**
- [ ] **Step 3: Implement README, user/developer docs, Apache-2.0 license, attribution/non-affiliation wording, security policy, contribution guidance, changelog, agent guidance, and benchmark limitations.
- [ ] **Step 4: Implement CI, release, security, issue-template, dependabot, and ignore metadata. Keep live tests opt-in.
- [ ] **Step 5: Run final verification in order: `cargo fmt --check`; `cargo clippy --all-targets --all-features -- -D warnings`; `cargo test --all`; `cargo build --release`; `target/release/ojg --help`; `target/release/ojg exact authenticate fixtures/sample-repo`; fake-backend JSON smoke test; `git diff --check`; secret-pattern scan; workflow YAML parse.
- [ ] **Step 6: Inspect `git status`, verify no generated secrets or placeholder paths, inspect README commands against the actual CLI, and commit `chore: prepare OpenJevGrep for release`.

## Final review and handoff

After Task 8, run the plan workspace/review procedure from `superpowers:executing-plans`, perform a fresh whole-branch review, fix Critical/Important findings with RED -> GREEN tests, and report only verification evidence that was actually observed. Live OpenJev compatibility is reported as not executed unless a running endpoint is explicitly available and tested.
