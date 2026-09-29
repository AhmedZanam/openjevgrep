use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{serve, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpListener;

use crate::backend::{Candidate, DecisionBackend, ModelInfo};
use crate::Result as AppResult;

const MODEL_ID: &str = "verdict-1.4";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            model: MODEL_ID.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SystemOneRequest {
    model: String,
    state: Value,
    questions: BTreeMap<String, NoulQuestion>,
}

#[derive(Debug, Deserialize)]
struct NoulQuestion {
    #[serde(rename = "type")]
    question_type: String,
    instructions: String,
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Serialize)]
struct SystemOneResponse {
    model: String,
    answers: BTreeMap<String, BTreeMap<String, f64>>,
    usage: Usage,
}

#[derive(Debug, Serialize)]
struct Usage {
    prompt_tokens: usize,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ErrorMessage,
}

#[derive(Debug, Serialize)]
struct ErrorMessage {
    message: String,
}

pub fn router(backend: Arc<dyn DecisionBackend>) -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/systemone", post(system_one))
        .with_state(backend)
}

pub async fn run_server(config: ServerConfig, backend: Arc<dyn DecisionBackend>) -> AppResult<()> {
    if config.model != MODEL_ID {
        return Err(anyhow::anyhow!("unsupported local model: {}", config.model));
    }
    let address: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = TcpListener::bind(address).await?;
    serve(listener, router(backend)).await?;
    Ok(())
}

async fn list_models(State(backend): State<Arc<dyn DecisionBackend>>) -> Response {
    match backend.models().await {
        Ok(data) => Json(ModelsResponse { data }).into_response(),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn system_one(
    State(backend): State<Arc<dyn DecisionBackend>>,
    payload: Result<Json<SystemOneRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(error) => return error_response(StatusCode::BAD_REQUEST, error.to_string()),
    };
    if request.model != MODEL_ID {
        return error_response(
            StatusCode::BAD_REQUEST,
            format!("unsupported model: {}", request.model),
        );
    }
    if request.questions.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "questions must not be empty");
    }
    let context = match request.state {
        Value::String(value) => value,
        value => match serde_json::to_string(&value) {
            Ok(value) => value,
            Err(error) => return error_response(StatusCode::BAD_REQUEST, error.to_string()),
        },
    };
    let mut candidates = Vec::with_capacity(request.questions.len());
    for (id, question) in &request.questions {
        if question.question_type != "noul" {
            return error_response(
                StatusCode::BAD_REQUEST,
                format!("question {id} must use type noul"),
            );
        }
        if question.instructions.trim().is_empty() {
            return error_response(
                StatusCode::BAD_REQUEST,
                format!("question {id} must include instructions"),
            );
        }
        candidates.push(Candidate {
            id: id.clone(),
            path: Default::default(),
            symbol: None,
            instructions: Some(question.instructions.clone()),
            start_line: 0,
            end_line: 0,
            content: question.instructions.clone(),
        });
    }
    let scores = match backend.score_batch(&context, &candidates).await {
        Ok(scores) => scores,
        Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let mut answers = BTreeMap::new();
    for score in scores {
        answers.insert(
            score.candidate_id,
            BTreeMap::from([("noul".to_string(), score.probability)]),
        );
    }
    Json(SystemOneResponse {
        model: MODEL_ID.to_string(),
        answers,
        usage: Usage { prompt_tokens: 0 },
    })
    .into_response()
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: ErrorMessage {
                message: message.into(),
            },
        }),
    )
        .into_response()
}
