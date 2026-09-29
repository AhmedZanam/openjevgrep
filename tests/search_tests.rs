use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ojg_core::backend::{BackendHealth, Candidate, DecisionBackend, ModelInfo, Score};
use ojg_core::cache::FileScoreCache;
use ojg_core::search::{search, search_with_cache, BackendMetadata, SearchOptions, SearchRequest};
use tempfile::tempdir;

struct FakeBackend {
    scores: HashMap<String, f64>,
    calls: Mutex<usize>,
    fail_after: Option<usize>,
}

#[async_trait]
impl DecisionBackend for FakeBackend {
    async fn health(&self) -> ojg_core::Result<BackendHealth> {
        Ok(BackendHealth {
            endpoint: "http://fake".to_string(),
            reachable: true,
            model: "fake".to_string(),
        })
    }

    async fn models(&self) -> ojg_core::Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: "fake".to_string(),
        }])
    }

    async fn score_batch(
        &self,
        _query: &str,
        candidates: &[Candidate],
    ) -> ojg_core::Result<Vec<Score>> {
        let mut calls = self.calls.lock().expect("calls");
        *calls += 1;
        if self.fail_after.is_some_and(|limit| *calls > limit) {
            return Err(anyhow::anyhow!("fake batch failure"));
        }
        Ok(candidates
            .iter()
            .map(|candidate| Score {
                candidate_id: candidate.id.clone(),
                probability: self.scores.get(&candidate.content).copied().unwrap_or(0.1),
            })
            .collect())
    }
}

fn request(root: &std::path::Path) -> SearchRequest {
    SearchRequest {
        query: "where is auth checked?".to_string(),
        root: root.to_path_buf(),
        scopes: Vec::new(),
        backend: BackendMetadata {
            endpoint: "http://fake".to_string(),
            model: "fake".to_string(),
        },
        options: SearchOptions {
            threshold: 0.70,
            limit: 20,
            batch_size: 16,
            max_file_size: 1024,
            max_chunk_lines: 2,
            context_lines: 1,
            ..SearchOptions::default()
        },
    }
}

#[tokio::test]
async fn filters_threshold_sorts_by_probability_and_preserves_source() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("low.rs"), "fn low() {}\n").expect("low");
    std::fs::write(root.path().join("high.rs"), "fn high() {}\n").expect("high");
    let backend = Arc::new(FakeBackend {
        scores: HashMap::from([
            ("fn low() {}".to_string(), 0.69),
            ("fn high() {}".to_string(), 0.91),
        ]),
        calls: Mutex::new(0),
        fail_after: None,
    });

    let response = search(request(root.path()), backend).await.expect("search");

    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].path, "high.rs");
    assert_eq!(response.results[0].probability, 0.91);
    assert_eq!(response.results[0].source, "fn high() {}");
}

#[tokio::test]
async fn merges_overlapping_chunks_and_keeps_strongest_probability() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("auth.rs"), "one\ntwo\nthree\n").expect("source");
    let backend = Arc::new(FakeBackend {
        scores: HashMap::new(),
        calls: Mutex::new(0),
        fail_after: None,
    });

    let mut options = request(root.path());
    options.options.threshold = 0.1;
    options.options.max_chunk_lines = 2;
    let response = search(options, backend).await.expect("search");

    assert_eq!(response.results.len(), 1);
    assert_eq!(
        (response.results[0].start_line, response.results[0].end_line),
        (1, 3)
    );
    assert_eq!(response.results[0].source, "one\ntwo\nthree");
}

#[tokio::test]
async fn partial_backend_failure_sets_coverage_and_keeps_successes() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("first.txt"), "first\n").expect("first");
    std::fs::write(root.path().join("second.txt"), "second\n").expect("second");
    let backend = Arc::new(FakeBackend {
        scores: HashMap::from([("first".to_string(), 0.9), ("second".to_string(), 0.9)]),
        calls: Mutex::new(0),
        fail_after: Some(1),
    });
    let mut request = request(root.path());
    request.options.batch_size = 1;

    let response = search(request, backend).await.expect("partial search");

    assert_eq!(response.results.len(), 1);
    assert!(response.coverage.partial);
    assert!(response.coverage.partial_reason.is_some());
}

#[tokio::test]
async fn score_cache_avoids_repeating_backend_batches() {
    let root = tempdir().expect("root");
    let cache_root = tempdir().expect("cache root");
    std::fs::write(root.path().join("auth.rs"), "fn auth() {}\n").expect("source");
    let cache = Arc::new(FileScoreCache::new(cache_root.path()).expect("cache"));
    let backend = Arc::new(FakeBackend {
        scores: HashMap::from([("fn auth() {}".to_string(), 0.9)]),
        calls: Mutex::new(0),
        fail_after: None,
    });

    let first = search_with_cache(request(root.path()), backend.clone(), cache.clone())
        .await
        .expect("first search");
    let second = search_with_cache(request(root.path()), backend.clone(), cache)
        .await
        .expect("cached search");

    assert_eq!(first.results, second.results);
    assert_eq!(*backend.calls.lock().expect("calls"), 1);
}
