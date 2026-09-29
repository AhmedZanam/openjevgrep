use ojg_core::backend::Score;
use ojg_core::cache::{CacheKey, FileScoreCache, ScoreCache};
use tempfile::tempdir;

fn key(candidate_hash: &str) -> CacheKey {
    CacheKey {
        endpoint: "http://127.0.0.1:8080".to_string(),
        model: "verdict-1.4".to_string(),
        query: "where is auth checked?".to_string(),
        candidate_hash: candidate_hash.to_string(),
    }
}

#[tokio::test]
async fn cache_hit_miss_and_invalidation_are_keyed_by_candidate_content() {
    let root = tempdir().expect("cache root");
    let cache = FileScoreCache::new(root.path()).expect("cache");
    let first = key("first");
    let second = key("second");
    let score = Score {
        candidate_id: "candidate-1".to_string(),
        probability: 0.91,
    };

    assert!(cache.get(&first).await.expect("miss").is_none());
    cache.put(&first, &score).await.expect("put");
    assert_eq!(cache.get(&first).await.expect("hit"), Some(score));
    assert!(cache.get(&second).await.expect("invalidated").is_none());
}

#[tokio::test]
async fn ignores_corrupt_entries_and_clears_atomically() {
    let root = tempdir().expect("cache root");
    let cache = FileScoreCache::new(root.path()).expect("cache");
    let key = key("corrupt");
    std::fs::write(cache.entry_path(&key), b"not json").expect("corrupt entry");
    assert!(cache.get(&key).await.expect("corrupt read").is_none());

    cache.clear().await.expect("clear");
    assert!(!cache.entry_path(&key).exists());
}

#[test]
fn cache_key_digest_is_stable_and_distinct() {
    let first = key("first");
    let second = key("second");
    assert_eq!(first.digest(), first.digest());
    assert_ne!(first.digest(), second.digest());
}
