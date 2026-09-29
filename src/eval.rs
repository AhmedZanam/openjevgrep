use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EvalTask {
    pub query: String,
    pub expected_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalMetrics {
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub mrr: f64,
}

pub fn calculate_metrics(tasks: &[EvalTask], rankings: &[Vec<String>]) -> EvalMetrics {
    if tasks.is_empty() {
        return EvalMetrics {
            recall_at_1: 0.0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            mrr: 0.0,
        };
    }
    let mut recall = [0.0; 3];
    let mut reciprocal_rank = 0.0;
    for (task, ranking) in tasks.iter().zip(rankings) {
        for (index, limit) in [1, 5, 10].into_iter().enumerate() {
            if ranking
                .iter()
                .take(limit)
                .any(|path| task.expected_files.iter().any(|expected| expected == path))
            {
                recall[index] += 1.0;
            }
        }
        if let Some(position) = ranking.iter().position(|path| {
            task.expected_files
                .iter()
                .any(|expected| expected == path)
        }) {
            reciprocal_rank += 1.0 / (position + 1) as f64;
        }
    }
    let count = tasks.len() as f64;
    EvalMetrics {
        recall_at_1: recall[0] / count,
        recall_at_5: recall[1] / count,
        recall_at_10: recall[2] / count,
        mrr: reciprocal_rank / count,
    }
}
