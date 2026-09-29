use ojg_core::backend::openjev::OpenJevBackend;
use ojg_core::backend::{Candidate, DecisionBackend};
use ojg_core::config::BackendConfig;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn backend_config(endpoint: String) -> BackendConfig {
    BackendConfig {
        endpoint,
        model: "verdict-1.4".to_string(),
        timeout_seconds: 2,
        concurrency: 2,
    }
}

fn candidate(id: &str, content: &str) -> Candidate {
    Candidate {
        id: id.to_string(),
        path: "src/auth.rs".into(),
        symbol: Some("validate".to_string()),
        start_line: 10,
        end_line: 12,
        content: content.to_string(),
    }
}

#[tokio::test]
async fn serializes_batched_noul_questions_and_parses_probabilities() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "verdict-1.4",
            "answers": {
                "candidate-1": {"noul": 0.92},
                "candidate-2": {"noul": 0.31}
            }
        })))
        .mount(&server)
        .await;
    let backend =
        OpenJevBackend::new(backend_config(server.uri()), reqwest::Client::new()).expect("backend");

    let scores = backend
        .score_batch(
            "where is authentication validated?",
            &[
                candidate("candidate-1", "fn validate() {}"),
                candidate("candidate-2", "fn log() {}"),
            ],
        )
        .await
        .expect("scores");

    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].candidate_id, "candidate-1");
    assert_eq!(scores[0].probability, 0.92);
    assert_eq!(scores[1].probability, 0.31);

    let requests = server.received_requests().await.expect("requests");
    let body: Value = serde_json::from_slice(&requests[0].body).expect("json body");
    assert_eq!(body["model"], "verdict-1.4");
    assert!(body["state"].as_str().unwrap().contains("authentication"));
    assert_eq!(body["questions"]["candidate-1"]["type"], "noul");
    assert!(body["questions"]["candidate-1"]["instructions"]
        .as_str()
        .unwrap()
        .contains("candidate-1"));
}

#[tokio::test]
async fn parses_model_catalog_and_health() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"id": "verdict-1.4"}, {"id": "laya-1.0"}]
        })))
        .mount(&server)
        .await;
    let backend =
        OpenJevBackend::new(backend_config(server.uri()), reqwest::Client::new()).expect("backend");

    let models = backend.models().await.expect("models");
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["verdict-1.4", "laya-1.0"]
    );
    assert!(backend.health().await.expect("health").reachable);
}

#[tokio::test]
async fn rejects_malformed_probability_answers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "verdict-1.4",
            "answers": {"candidate-1": {"noul": 1.4}}
        })))
        .mount(&server)
        .await;
    let backend =
        OpenJevBackend::new(backend_config(server.uri()), reqwest::Client::new()).expect("backend");

    let error = backend
        .score_batch("query", &[candidate("candidate-1", "source")])
        .await
        .expect_err("out-of-range probability");
    assert!(error.to_string().contains("probability"));
}

#[tokio::test]
async fn retries_retryable_server_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "verdict-1.4",
            "answers": {"candidate-1": {"noul": 0.8}}
        })))
        .mount(&server)
        .await;
    let backend =
        OpenJevBackend::new(backend_config(server.uri()), reqwest::Client::new()).expect("backend");

    let scores = backend
        .score_batch("query", &[candidate("candidate-1", "source")])
        .await
        .expect("retry succeeds");
    assert_eq!(scores[0].probability, 0.8);
}
