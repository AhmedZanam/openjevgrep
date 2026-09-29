use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;

use crate::Result;

pub mod openjev;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: String,
    pub path: PathBuf,
    pub symbol: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub candidate_id: String,
    pub probability: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendHealth {
    pub endpoint: String,
    pub reachable: bool,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelInfo {
    pub id: String,
}

#[async_trait]
pub trait DecisionBackend: Send + Sync {
    async fn health(&self) -> Result<BackendHealth>;
    async fn models(&self) -> Result<Vec<ModelInfo>>;
    async fn score_batch(&self, query: &str, candidates: &[Candidate]) -> Result<Vec<Score>>;
}
