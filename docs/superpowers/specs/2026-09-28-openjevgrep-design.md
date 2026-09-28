# OpenJevGrep Architecture Design

## Status

Approved design for the initial implementation of `openjevgrep` / `ojg`.

## Purpose and success criteria

OpenJevGrep is a local-first semantic grep tool for source repositories. It answers natural-language questions by sending deterministic source candidates to an OpenJev-compatible System One decision server and returning the original repository text with exact line ranges.

The first release is successful when a user can:

- run `ojg "where is authentication checked?" [PATH]` against a local repository;
- receive ranked results whose public probability is the value returned by OpenJev;
- use exact search, JSON output, compact output, files-only output, and MCP over stdio;
- configure endpoint, model, limits, ignores, and cache behavior without an index or cloud service;
- run the full test suite without downloading model weights or requiring a live OpenJev server.

The implementation must not generate replacement source, use embeddings as a requirement, persist a vector database, or upload repository metadata through telemetry.

## Deliberate scope

The repository is empty, so the implementation is staged as one coherent Rust package rather than multiple independently published crates. The package will contain a reusable library and two binaries:

- `ojg`: end-user CLI, including the MCP stdio subcommand;
- `ojg-eval`: deterministic retrieval evaluation runner.

The library boundaries remain explicit so a later workspace split or alternate backend can be added without changing the search model. Cloud providers, an embedded model, watch mode, an LSP, persistent indexing, and optional embedding prefilters are future extension points, not first-release work.

## Architecture

```text
CLI / MCP
   |
   v
Config resolution -> Repository scanner -> Source chunker
                                           |
                                           v
                         Candidate preparation and size control
                                           |
                         +-----------------+-----------------+
                         |                                   |
                    cache lookup                       OpenJev backend
                         |                                   |
                         +-----------------+-----------------+
                                           v
                         threshold -> deterministic ranking
                                           v
                                adjacent-result merging
                                           v
                         original source + output formatter
```

### Core modules

- `config`: typed built-in, user, project, and CLI settings with precedence and environment overrides.
- `scanner`: repository-relative file discovery, ignore matching, UTF-8/binary checks, size limits, secret-file exclusions, and scan diagnostics.
- `chunk`: language detection, Tree-sitter extraction for Rust/Python/JavaScript/TypeScript/TSX/Java/Go, deterministic fallback chunks, source locations, symbols, and oversized-declaration splitting.
- `backend`: `DecisionBackend` trait plus `OpenJevBackend`, request/response types, health/model calls, timeout/retry behavior, bounded request concurrency, and strict probability parsing.
- `cache`: content-addressed decision records keyed by backend endpoint/model, query, candidate identity, and relevant configuration; writes are atomic and corrupt entries are ignored.
- `search`: candidate preparation, conservative hierarchical retrieval, batching, cancellation, thresholding, deterministic sort, merge, coverage, and timing statistics.
- `output`: human, compact, files-only, and versioned JSON contracts. Diagnostics and progress go to stderr; JSON data stays on stdout.
- `exact`: `rg` execution when available and a built-in recursive literal-search fallback.
- `mcp`: JSON-RPC stdio server exposing `semantic_search_code` without vendor-specific assumptions.

## OpenJev integration

The primary backend targets the current OpenJev-compatible contract:

- `POST /v1/systemone` with `{ state, model, questions }`;
- `GET /v1/models` for discovery;
- a relevance question represented as a `noul` decision;
- the returned `answers[question_id].noul` value is the only public semantic probability.

The client will not depend on OpenJev Python internals. The default endpoint is `http://127.0.0.1:8080`, overridden by CLI, `OPENJEV_URL`, project/user configuration, or the built-in default. The default model is `verdict-1.4`; arbitrary model strings are accepted, including `laya-1.0`.

Each request state contains the user query and clearly delimited candidate metadata/source. A batch contains multiple candidate-specific `noul` questions. The request builder estimates payload size and reduces batch size or splits candidates before sending, so small-context models do not crash the search. The initial implementation uses conservative defaults suited to CPU-local servers and a semaphore for bounded concurrency.

HTTP errors are classified into actionable configuration/model errors, retryable overload/rate-limit errors, timeout errors, and malformed-response errors. Partial evaluation is represented in coverage and never described as evidence that an unscanned candidate is irrelevant.

## Retrieval behavior

Files below configured limits are scanned directly. For fewer than the configured chunk threshold (default 150), chunks are scored directly. At or above the threshold, deterministic file previews are scored first; previews contain path, language, imports, top-level symbols, declaration signatures, and first meaningful lines. No model-generated summary is used. File pruning is conservative to favor recall, and all eligible chunks in qualifying files are then evaluated.

Candidates are scored in batches. Results are filtered by the configured threshold (default `0.70`), sorted by probability descending, then path and start line ascending. Overlapping or directly adjacent chunks in the same file are merged while retaining the strongest returned probability. Source is reread from the repository for display and JSON output, never reconstructed from model output.

Coverage reports files scanned, chunks found, chunks evaluated, partial status, and the reason for any incomplete evaluation. Timing reports scan, evaluation, and total duration.

## Safety and privacy

The scanner respects `.gitignore`, `.ignore`, and `.ojgignore`, ignores `.git` and common build/dependency directories by default, rejects binary/non-UTF-8 files, skips symlinks unless explicitly followed, and excludes likely secret files such as `.env*`, private keys, credentials, and secret manifests before semantic evaluation. Explicit include patterns may override safe directory defaults but do not silently disable secret protection.

The default endpoint is local. If a remote endpoint is configured, selected source fragments are sent there. The cache stores decision metadata and candidate hashes/source identity; cache contents and location are documented. There is no telemetry, analytics, crash reporting, or repository metadata upload.

## Configuration

Resolution order is:

```text
CLI > environment > project .openjevgrep.toml > user config > built-in defaults
```

The user config follows platform application-data conventions. Project configuration is optional and must not contain credentials. Settings cover endpoint, model, timeout, threshold, limit, batch size, concurrency, file/chunk limits, context lines, cache enablement, ignores, and hierarchical threshold. Invalid values fail with field-specific messages.

## CLI and MCP surface

The parser supports the shorthand search form and explicit `search`, with repeated `--scope` and the requested filtering/output flags. First-release subcommands are `init`, `doctor`, `inspect`, `exact`, `models`, `config`, `cache`, `mcp`, and `completions`.

`doctor` checks repository/configuration/backend/model/cache/`rg` availability and provides actionable, non-installing suggestions. `inspect` reports included/excluded files, reasons, chunks, estimated requests, and estimated input size. `init` is non-interactive and prints the detected repository, endpoint, models, selected default, and an exact next command.

The MCP server uses newline-delimited JSON-RPC over stdin/stdout, keeps logs on stderr, validates tool arguments, and maps search coverage/errors into structured responses.

## Testing and verification strategy

- Unit tests cover config precedence/validation, ignore and binary handling, secret exclusions, chunk ranges/symbols/fallbacks, OpenJev serialization/parsing/errors, cache hit/miss/invalidation, ranking/merge/threshold, and all output modes.
- Integration tests use a local fake HTTP server and a fixture repository. They verify ranked results, exact source/line ranges, partial backend failures, model listing, and CLI behavior without network or model weights.
- Optional live tests are feature-gated and run only when `OPENJEV_LIVE_URL` is supplied.
- `fixtures/` and `eval/` contain deterministic behaviors and annotated questions; `ojg-eval` calculates Recall@1, Recall@5, Recall@10, and MRR without claiming quality beyond the measured fixture.
- Benchmarks measure scan, AST parsing, chunking, cache lookup, backend evaluation, and end-to-end search independently. CI does not enforce hardware-sensitive timing thresholds.
- Verification commands are `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all`, `cargo build --release`, and CLI/fake-backend smoke tests.

## Delivery stages

1. Bootstrap the package, CLI, configuration, scanner, fallback chunker, backend, basic search, output, and tests.
2. Add Tree-sitter chunking, symbols, accurate locations, and oversized candidate handling.
3. Add hierarchical retrieval, batching, bounded concurrency, ranking, merging, and coverage.
4. Add cache, metrics, benchmarks, fixtures, and the evaluation runner.
5. Add the complete CLI workflow (`init`, `doctor`, `inspect`, `models`, `exact`, compact/files/JSON, completions).
6. Add MCP stdio support and integration tests.
7. Add README, architecture/config/OpenJev/MCP/security/benchmark documentation, Apache-2.0 licensing, CI, release workflow, issue templates, and changelog.

Each stage must leave the repository buildable and testable. Commits use conventional messages and are made only after the stage's verification commands pass.

## Risks and mitigations

- OpenJev deployments may expose different model catalogs or context limits: use arbitrary model names, parse server errors, keep payload limits configurable, and test the documented wire shape with a fake server.
- AST grammars may disagree on node shapes across crate versions: isolate grammar adapters and preserve the deterministic fallback for every language.
- Large repositories can overwhelm a CPU-local server: use conservative hierarchical activation, batching, semaphore limits, cache reuse, cancellation, and explicit partial coverage.
- Ignore rules can accidentally hide relevant code: make defaults visible through `inspect`, honor standard ignore files, and keep pruning conservative.
- Secret detection is necessarily heuristic: exclude high-confidence patterns by default, document behavior, and never claim complete secret detection.
