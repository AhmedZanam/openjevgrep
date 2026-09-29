use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::backend::{Candidate, DecisionBackend};
use crate::cache::{CacheKey, ScoreCache};
use crate::chunk::tree_sitter::{deterministic_file_preview, source_chunks, ChunkOptions};
use crate::chunk::{ChunkKind, SourceChunk};
use crate::retrieval::uses_hierarchical_retrieval;
use crate::scanner::{scan_repository, ScanOptions};
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendMetadata {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub root: PathBuf,
    pub scopes: Vec<PathBuf>,
    pub backend: BackendMetadata,
    pub options: SearchOptions,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub threshold: f64,
    pub limit: usize,
    pub batch_size: usize,
    pub concurrency: usize,
    pub max_file_size: u64,
    pub max_chunk_lines: usize,
    pub context_lines: usize,
    pub hierarchical_threshold: usize,
    pub scan: ScanOptions,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            threshold: 0.70,
            limit: 20,
            batch_size: 16,
            concurrency: 4,
            max_file_size: 1_048_576,
            max_chunk_lines: 160,
            context_lines: 2,
            hierarchical_threshold: 150,
            scan: ScanOptions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub symbol: Option<String>,
    pub kind: String,
    pub probability: f64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Coverage {
    pub files_scanned: usize,
    pub chunks_found: usize,
    pub chunks_evaluated: usize,
    pub partial: bool,
    pub partial_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Timing {
    pub scan_ms: u64,
    pub evaluation_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResponse {
    pub query: String,
    pub root: PathBuf,
    pub backend: BackendMetadata,
    pub results: Vec<SearchResult>,
    pub coverage: Coverage,
    pub timing: Timing,
}

#[derive(Debug, Clone)]
struct CandidateRecord {
    candidate: Candidate,
    chunk: SourceChunk,
}

pub async fn search(
    request: SearchRequest,
    backend: Arc<dyn DecisionBackend>,
) -> Result<SearchResponse> {
    search_internal(request, backend, None).await
}

pub async fn search_with_cache(
    request: SearchRequest,
    backend: Arc<dyn DecisionBackend>,
    cache: Arc<dyn ScoreCache>,
) -> Result<SearchResponse> {
    search_internal(request, backend, Some(cache)).await
}

async fn search_internal(
    request: SearchRequest,
    backend: Arc<dyn DecisionBackend>,
    cache: Option<Arc<dyn ScoreCache>>,
) -> Result<SearchResponse> {
    let total_start = Instant::now();
    let scan_start = Instant::now();
    let scan_options = ScanOptions {
        max_file_size: request.options.max_file_size,
        ..request.options.scan.clone()
    };
    let scan = scan_repository(&request.root, &scan_options)?;
    let scoped_files: Vec<_> = scan
        .included
        .iter()
        .filter(|file| in_scope(&request.root, &file.relative_path, &request.scopes))
        .collect();
    let scan_ms = scan_start.elapsed().as_millis() as u64;

    let mut records = Vec::new();
    let mut source_by_path = HashMap::new();
    let mut chunks_by_path = HashMap::new();
    let mut files_by_path = HashMap::new();
    for file in scoped_files {
        let path = path_string(&file.relative_path);
        source_by_path.insert(path.clone(), file.content.clone());
        files_by_path.insert(path.clone(), file.clone());
        let chunks = source_chunks(
            file,
            &ChunkOptions {
                max_lines: request.options.max_chunk_lines,
                context_lines: request.options.context_lines,
            },
        )?;
        chunks_by_path.insert(path, chunks.clone());
        for (index, chunk) in chunks.into_iter().enumerate() {
            let id = format!(
                "{}:{}-{}:{}",
                path_string(&chunk.path),
                chunk.start_line,
                chunk.end_line,
                index
            );
            records.push(CandidateRecord {
                candidate: Candidate {
                    id,
                    path: chunk.path.clone(),
                    symbol: chunk.symbol.clone(),
                    instructions: None,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    content: chunk.content.clone(),
                },
                chunk,
            });
        }
    }

    let evaluation_start = Instant::now();
    let mut partial_reasons = Vec::new();
    let mut evaluated = 0;
    let mut raw_results = Vec::new();
    let batch_size = request.options.batch_size.max(1);
    let evaluation_records =
        if uses_hierarchical_retrieval(records.len(), request.options.hierarchical_threshold) {
            let file_candidates: Vec<_> = chunks_by_path
                .iter()
                .map(|(path, chunks)| Candidate {
                    id: format!("__file__:{path}"),
                    path: PathBuf::from(path),
                    symbol: None,
                    instructions: None,
                    start_line: 1,
                    end_line: 1,
                    content: deterministic_file_preview(
                        files_by_path.get(path).expect("file preview source"),
                        chunks,
                        4_096,
                    ),
                })
                .collect();
            let mut selected_paths = HashSet::new();
            let mut best_file = None;
            let mut coarse_failed = false;
            for batch in file_candidates.chunks(batch_size) {
                match score_batch_cached(&request, &backend, cache.as_ref(), batch).await {
                    Ok(scores) => {
                        for score in scores {
                            let path = score.candidate_id.trim_start_matches("__file__:");
                            if best_file.as_ref().is_none_or(|(_, best_probability)| {
                                score.probability > *best_probability
                            }) {
                                best_file = Some((path.to_string(), score.probability));
                            }
                            if score.probability >= request.options.threshold {
                                selected_paths.insert(path.to_string());
                            }
                        }
                    }
                    Err(error) => {
                        coarse_failed = true;
                        partial_reasons.push(error.to_string());
                    }
                }
            }
            if !coarse_failed {
                if selected_paths.is_empty() {
                    if let Some((path, _)) = best_file {
                        selected_paths.insert(path);
                    }
                }
                records
                    .iter()
                    .filter(|record| selected_paths.contains(&path_string(&record.chunk.path)))
                    .cloned()
                    .collect()
            } else {
                records.clone()
            }
        } else {
            records.clone()
        };
    for batch in evaluation_records.chunks(batch_size) {
        let candidates: Vec<_> = batch
            .iter()
            .map(|record| record.candidate.clone())
            .collect();
        match score_batch_cached(&request, &backend, cache.as_ref(), &candidates).await {
            Ok(scores) => {
                evaluated += scores.len();
                for score in scores {
                    if let Some(record) = batch
                        .iter()
                        .find(|record| record.candidate.id == score.candidate_id)
                    {
                        if score.probability >= request.options.threshold {
                            raw_results.push(RawResult {
                                path: path_string(&record.chunk.path),
                                start_line: record.chunk.start_line,
                                end_line: record.chunk.end_line,
                                symbol: record.chunk.symbol.clone(),
                                kind: chunk_kind(&record.chunk.kind).to_string(),
                                probability: score.probability,
                            });
                        }
                    }
                }
            }
            Err(error) => partial_reasons.push(error.to_string()),
        }
    }
    let evaluation_ms = evaluation_start.elapsed().as_millis() as u64;

    let merged = merge_results(raw_results, &source_by_path);
    let mut results = merged;
    results.sort_by(|left, right| {
        right
            .probability
            .total_cmp(&left.probability)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.start_line.cmp(&right.start_line))
    });
    results.truncate(request.options.limit.max(1));

    Ok(SearchResponse {
        query: request.query,
        root: request.root,
        backend: request.backend,
        results,
        coverage: Coverage {
            files_scanned: source_by_path.len(),
            chunks_found: records.len(),
            chunks_evaluated: evaluated,
            partial: !partial_reasons.is_empty(),
            partial_reason: (!partial_reasons.is_empty()).then(|| partial_reasons.join("; ")),
        },
        timing: Timing {
            scan_ms,
            evaluation_ms,
            total_ms: total_start.elapsed().as_millis() as u64,
        },
    })
}

async fn score_batch_cached(
    request: &SearchRequest,
    backend: &Arc<dyn DecisionBackend>,
    cache: Option<&Arc<dyn ScoreCache>>,
    candidates: &[Candidate],
) -> Result<Vec<crate::backend::Score>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let Some(cache) = cache else {
        return backend.score_batch(&request.query, candidates).await;
    };

    let mut scores = Vec::with_capacity(candidates.len());
    let mut misses = Vec::new();
    for candidate in candidates {
        let key = cache_key(request, candidate);
        match cache.get(&key).await {
            Ok(Some(mut score)) => {
                score.candidate_id = candidate.id.clone();
                scores.push(score);
            }
            Ok(None) | Err(_) => misses.push((candidate, key)),
        }
    }

    if misses.is_empty() {
        return Ok(scores);
    }

    let missing_candidates: Vec<_> = misses
        .iter()
        .map(|(candidate, _)| (*candidate).clone())
        .collect();
    let fetched = backend
        .score_batch(&request.query, &missing_candidates)
        .await?;
    for score in fetched {
        if let Some((_, key)) = misses
            .iter()
            .find(|(candidate, _)| candidate.id == score.candidate_id)
        {
            let _ = cache.put(key, &score).await;
        }
        scores.push(score);
    }
    Ok(scores)
}

fn cache_key(request: &SearchRequest, candidate: &Candidate) -> CacheKey {
    let mut hasher = Sha256::new();
    hasher.update(candidate.id.as_bytes());
    hasher.update([0]);
    hasher.update(candidate.path.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(candidate.start_line.to_le_bytes());
    hasher.update(candidate.end_line.to_le_bytes());
    hasher.update(candidate.symbol.as_deref().unwrap_or_default().as_bytes());
    hasher.update([0]);
    hasher.update(candidate.content.as_bytes());
    CacheKey {
        endpoint: request.backend.endpoint.clone(),
        model: request.backend.model.clone(),
        query: request.query.clone(),
        candidate_hash: format!("{:x}", hasher.finalize()),
    }
}

#[derive(Debug)]
struct RawResult {
    path: String,
    start_line: usize,
    end_line: usize,
    symbol: Option<String>,
    kind: String,
    probability: f64,
}

fn merge_results(
    raw_results: Vec<RawResult>,
    source_by_path: &HashMap<String, String>,
) -> Vec<SearchResult> {
    let mut sorted = raw_results;
    sorted.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| right.probability.total_cmp(&left.probability))
    });
    let mut merged: Vec<RawResult> = Vec::new();
    for result in sorted {
        if let Some(previous) = merged.last_mut() {
            if previous.path == result.path && result.start_line <= previous.end_line + 1 {
                previous.end_line = previous.end_line.max(result.end_line);
                if result.probability > previous.probability {
                    previous.probability = result.probability;
                    previous.symbol = result.symbol.clone();
                    previous.kind = result.kind.clone();
                }
                continue;
            }
        }
        merged.push(result);
    }
    merged
        .into_iter()
        .map(|result| SearchResult {
            source: source_by_path
                .get(&result.path)
                .map(|source| source_range(source, result.start_line, result.end_line))
                .unwrap_or_default(),
            path: result.path,
            start_line: result.start_line,
            end_line: result.end_line,
            symbol: result.symbol,
            kind: result.kind,
            probability: result.probability,
        })
        .collect()
}

fn source_range(source: &str, start_line: usize, end_line: usize) -> String {
    let mut lines: Vec<&str> = source.split('\n').collect();
    if source.ends_with('\n') {
        lines.pop();
    }
    lines
        .get(start_line.saturating_sub(1)..end_line.min(lines.len()))
        .unwrap_or(&[])
        .iter()
        .map(|line| line.trim_end_matches('\r'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn in_scope(root: &Path, relative: &Path, scopes: &[PathBuf]) -> bool {
    if scopes.is_empty() {
        return true;
    }
    scopes.iter().any(|scope| {
        let scope = if scope.is_absolute() {
            scope
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| scope.clone())
        } else {
            scope.clone()
        };
        relative.starts_with(scope)
    })
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn chunk_kind(kind: &ChunkKind) -> &'static str {
    match kind {
        ChunkKind::Paragraph => "paragraph",
        ChunkKind::Lines => "lines",
        ChunkKind::Function => "function",
        ChunkKind::Method => "method",
        ChunkKind::Class => "class",
        ChunkKind::Declaration => "declaration",
    }
}
