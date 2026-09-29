use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use super::{BackendHealth, Candidate, DecisionBackend, ModelInfo, Score};
use crate::config::BackendConfig;
use crate::Result;

pub struct OpenJevBackend {
    endpoint: String,
    model: String,
    timeout: Duration,
    client: Client,
}

impl OpenJevBackend {
    pub fn new(config: BackendConfig, client: Client) -> Result<Self> {
        let endpoint = config.endpoint.trim_end_matches('/').to_string();
        reqwest::Url::parse(&endpoint)?;
        Ok(Self {
            endpoint,
            model: config.model,
            timeout: Duration::from_secs(config.timeout_seconds),
            client,
        })
    }

    async fn get_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/v1/models", self.endpoint);
        let response = self
            .client
            .get(url)
            .timeout(self.timeout)
            .send()
            .await?;
        let response = ensure_success(response).await?;
        let payload: ModelsPayload = response.json().await?;
        Ok(payload.into_models())
    }

    async fn post_decisions(&self, request: &SystemOneRequest) -> Result<SystemOneResponse> {
        let url = format!("{}/v1/systemone", self.endpoint);
        let mut attempt = 0;
        loop {
            let response = self
                .client
                .post(&url)
                .timeout(self.timeout)
                .json(request)
                .send()
                .await;
            match response {
                Ok(response) if is_retryable(response.status()) && attempt < 2 => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(25 * attempt)).await;
                }
                Ok(response) => {
                    let response = ensure_success(response).await?;
                    return Ok(response.json().await?);
                }
                Err(error) if error.is_timeout() && attempt < 2 => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(25 * attempt)).await;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

#[async_trait]
impl DecisionBackend for OpenJevBackend {
    async fn health(&self) -> Result<BackendHealth> {
        self.get_models().await?;
        Ok(BackendHealth {
            endpoint: self.endpoint.clone(),
            reachable: true,
            model: self.model.clone(),
        })
    }

    async fn models(&self) -> Result<Vec<ModelInfo>> {
        self.get_models().await
    }

    async fn score_batch(&self, query: &str, candidates: &[Candidate]) -> Result<Vec<Score>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let mut questions = BTreeMap::new();
        for candidate in candidates {
            questions.insert(
                candidate.id.clone(),
                NoulQuestion {
                    question_type: "noul",
                    instructions: format!(
                        "Does candidate {} contain information that helps answer the user's repository search question?",
                        candidate.id
                    ),
                    criteria: BTreeMap::from([
                        (
                            "true".to_string(),
                            "The source directly contains or materially helps locate the requested behavior.".to_string(),
                        ),
                        (
                            "false".to_string(),
                            "The source is unrelated or only shares incidental vocabulary.".to_string(),
                        ),
                    ]),
                },
            );
        }
        let mut state = format!("User repository search question:\n{query}\n\nSource candidates:\n");
        for candidate in candidates {
            state.push_str("---\n");
            state.push_str("candidate: ");
            state.push_str(&candidate.id);
            state.push_str("\npath: ");
            state.push_str(&candidate.path.to_string_lossy());
            if let Some(symbol) = &candidate.symbol {
                state.push_str("\nsymbol: ");
                state.push_str(symbol);
            }
            state.push_str("\nsource:\n");
            state.push_str(&candidate.content);
            state.push('\n');
        }
        let response = self
            .post_decisions(&SystemOneRequest {
                model: &self.model,
                state,
                questions,
            })
            .await?;
        let mut scores = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let answer = response
                .answers
                .get(&candidate.id)
                .ok_or_else(|| anyhow::anyhow!("missing probability for candidate {}", candidate.id))?;
            let probability = answer
                .noul
                .ok_or_else(|| anyhow::anyhow!("missing probability for candidate {}", candidate.id))?;
            if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
                return Err(anyhow::anyhow!(
                    "probability for candidate {} is outside [0, 1]",
                    candidate.id
                ));
            }
            scores.push(Score {
                candidate_id: candidate.id.clone(),
                probability,
            });
        }
        Ok(scores)
    }
}

#[derive(Debug, Serialize)]
struct SystemOneRequest<'a> {
    model: &'a str,
    state: String,
    questions: BTreeMap<String, NoulQuestion>,
}

#[derive(Debug, Serialize)]
struct NoulQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: String,
    criteria: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    answers: BTreeMap<String, NoulAnswer>,
}

#[derive(Debug, Deserialize)]
struct NoulAnswer {
    noul: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ModelsPayload {
    Data { data: Vec<ModelInfo> },
    Models { models: Vec<ModelInfo> },
    List(Vec<ModelInfo>),
}

impl ModelsPayload {
    fn into_models(self) -> Vec<ModelInfo> {
        match self {
            Self::Data { data } | Self::Models { models: data } | Self::List(data) => data,
        }
    }
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(anyhow::anyhow!("OpenJev returned HTTP {status}: {body}"))
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
        || status.is_server_error()
}
