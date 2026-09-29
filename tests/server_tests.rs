use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use ojg_core::backend::{BackendHealth, Candidate, DecisionBackend, ModelInfo, Score};
use ojg_core::server::router;
use ojg_core::Result;
use serde_json::Value;
use tower::ServiceExt;

struct FakeBackend {
    failure: bool,
}

#[async_trait]
impl DecisionBackend for FakeBackend {
    async fn health(&self) -> Result<BackendHealth> {
        Ok(BackendHealth {
            endpoint: "local://test".to_string(),
            reachable: true,
            model: "verdict-1.4".to_string(),
        })
    }

    async fn models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: "verdict-1.4".to_string(),
        }])
    }

    async fn score_batch(&self, _query: &str, candidates: &[Candidate]) -> Result<Vec<Score>> {
        if self.failure {
            return Err(anyhow::anyhow!("inference failed"));
        }
        if candidates.len() == 2 {
            assert_eq!(
                candidates[0].instructions.as_deref(),
                Some("Does candidate-1 help?")
            );
            assert_eq!(
                candidates[1].instructions.as_deref(),
                Some("Does candidate-2 help?")
            );
        }
        Ok(candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| Score {
                candidate_id: candidate.id.clone(),
                probability: 0.2 + index as f64 * 0.1,
            })
            .collect())
    }
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

#[tokio::test]
async fn lists_the_local_model() {
    let app = router(Arc::new(FakeBackend { failure: false }));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await["data"][0]["id"],
        "verdict-1.4"
    );
}

#[tokio::test]
async fn converts_noul_questions_and_preserves_ids() {
    let app = router(Arc::new(FakeBackend { failure: false }));
    let request = serde_json::json!({
        "model": "verdict-1.4",
        "state": "find authentication",
        "questions": {
            "candidate-1": {
                "type": "noul",
                "instructions": "Does candidate-1 help?",
                "criteria": {"true": "yes", "false": "no"}
            },
            "candidate-2": {
                "type": "noul",
                "instructions": "Does candidate-2 help?",
                "criteria": {"true": "yes", "false": "no"}
            }
        }
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    assert_eq!(json["answers"]["candidate-1"]["noul"], 0.2);
    assert!((json["answers"]["candidate-2"]["noul"].as_f64().unwrap() - 0.3).abs() < 1e-12);
}

#[tokio::test]
async fn rejects_invalid_model_and_malformed_questions() {
    let app = router(Arc::new(FakeBackend { failure: false }));
    let invalid_model = serde_json::json!({
        "model": "other",
        "state": "query",
        "questions": {}
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(invalid_model.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let malformed = serde_json::json!({
        "model": "verdict-1.4",
        "state": "query",
        "questions": {"candidate-1": {"type": "choice"}}
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(malformed.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn returns_structured_backend_failure() {
    let app = router(Arc::new(FakeBackend { failure: true }));
    let request = serde_json::json!({
        "model": "verdict-1.4",
        "state": "query",
        "questions": {"candidate-1": {"type": "noul", "instructions": "help?"}}
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/systemone")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response_json(response).await["error"]["message"],
        "inference failed"
    );
}
