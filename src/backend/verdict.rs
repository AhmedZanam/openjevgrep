use std::collections::BTreeMap;
use std::sync::Mutex;

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use ort::{session::Session, value::Tensor};
use serde::Deserialize;
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use super::{BackendHealth, Candidate, DecisionBackend, ModelInfo, Score};
use crate::model::ModelFiles;
use crate::Result;

pub const VERDICT_MAX_LEN: usize = 512;
const ABSTAIN: &str = "insufficient evidence";
const LABEL_MARKER: &str = "<<LABEL>>";
const SEP_MARKER: &str = "<<SEP>>";

#[derive(Debug, Clone, Deserialize)]
pub struct Calibrator {
    pub temperature: f64,
    #[serde(default)]
    pub per_k: BTreeMap<usize, f64>,
}

pub struct LocalVerdictBackend {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    calibrator: Calibrator,
}

pub fn prompt_for_noul(instructions: &str, context: &str) -> (String, usize) {
    let labels = [
        format!("true: {instructions}"),
        format!("false: not {instructions}"),
        ABSTAIN.to_string(),
    ];
    let prompt = format!(
        "{}{}Context:\n{context}\n\nEvaluate proposition: {instructions}",
        labels
            .iter()
            .map(|label| format!("{LABEL_MARKER}{label}"))
            .collect::<String>(),
        SEP_MARKER
    );
    (prompt, labels.len())
}

pub fn temperature_for(label_count: usize, calibrator: &Calibrator) -> f64 {
    calibrator
        .per_k
        .get(&label_count)
        .copied()
        .unwrap_or(calibrator.temperature)
}

pub fn calibrated_probabilities(
    logits: &[f32],
    label_count: usize,
    calibrator: &Calibrator,
) -> Result<Vec<f64>> {
    if label_count < 2 || logits.len() < label_count {
        return Err(anyhow!("Verdict output has fewer logits than labels"));
    }
    let temperature = temperature_for(label_count, calibrator);
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(anyhow!("Verdict calibration temperature must be positive"));
    }
    let scaled: Vec<f64> = logits[..label_count]
        .iter()
        .map(|logit| f64::from(*logit) / temperature)
        .collect();
    let maximum = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = scaled
        .iter()
        .map(|value| (*value - maximum).exp())
        .collect();
    let denominator: f64 = weights.iter().sum();
    if !denominator.is_finite() || denominator <= 0.0 {
        return Ok(vec![1.0 / (label_count - 1) as f64; label_count - 1]);
    }
    let mut probabilities: Vec<f64> = weights[..label_count - 1]
        .iter()
        .map(|weight| weight / denominator)
        .collect();
    let caller_sum: f64 = probabilities.iter().sum();
    if !caller_sum.is_finite() || caller_sum <= 0.0 {
        return Ok(vec![1.0 / (label_count - 1) as f64; label_count - 1]);
    }
    probabilities
        .iter_mut()
        .for_each(|value| *value /= caller_sum);
    Ok(probabilities)
}

pub fn truncate_tokens(tokens: &[u32], max_length: usize) -> Vec<u32> {
    tokens.iter().copied().take(max_length).collect()
}

impl LocalVerdictBackend {
    pub fn load(files: ModelFiles) -> Result<Self> {
        let mut tokenizer = Tokenizer::from_file(&files.tokenizer_path)
            .map_err(|error| anyhow!("loading tokenizer: {error}"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: VERDICT_MAX_LEN,
                ..Default::default()
            }))
            .map_err(|error| anyhow!("configuring tokenizer truncation: {error}"))?;
        tokenizer.with_padding(Some(PaddingParams::default()));
        let calibrator: Calibrator =
            serde_json::from_slice(&std::fs::read(&files.calibrator_path)?)
                .context("parsing Verdict calibration")?;
        let session = Session::builder()?
            .commit_from_file(&files.model_path)
            .context("loading Verdict ONNX model")?;
        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            calibrator,
        })
    }

    fn score_sync(&self, query: &str, candidates: &[Candidate]) -> Result<Vec<Score>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let prompts: Vec<String> = candidates
            .iter()
            .map(|candidate| {
                let instructions = candidate.instructions.clone().unwrap_or_else(|| {
                    format!(
                        "Does candidate {} contain information that helps answer the user's repository search question?",
                        candidate.id
                    )
                });
                prompt_for_noul(&instructions, query).0
            })
            .collect();
        let encodings = self
            .tokenizer
            .encode_batch(prompts.iter().map(String::as_str).collect(), true)
            .map_err(|error| anyhow!("encoding Verdict prompts: {error}"))?;
        let sequence_length = encodings
            .first()
            .map(|encoding| encoding.len())
            .ok_or_else(|| anyhow!("Verdict tokenizer returned no encodings"))?;
        if sequence_length == 0 {
            return Err(anyhow!("Verdict tokenizer returned an empty encoding"));
        }
        let mut input_ids = Vec::with_capacity(encodings.len() * sequence_length);
        let mut attention_mask = Vec::with_capacity(input_ids.capacity());
        for encoding in &encodings {
            if encoding.len() != sequence_length {
                return Err(anyhow!(
                    "Verdict tokenizer did not pad the batch consistently"
                ));
            }
            input_ids.extend(encoding.get_ids().iter().map(|token| i64::from(*token)));
            attention_mask.extend(
                encoding
                    .get_attention_mask()
                    .iter()
                    .map(|value| i64::from(*value)),
            );
        }
        let shape = vec![encodings.len(), sequence_length];
        let input_ids = Tensor::<i64>::from_array((shape.clone(), input_ids))?;
        let attention_mask = Tensor::<i64>::from_array((shape, attention_mask))?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow!("Verdict session lock poisoned"))?;
        let outputs = session.run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
        ])?;
        let (_, logits) = outputs[0].try_extract_tensor::<f32>()?;
        if logits.len() % candidates.len() != 0 {
            return Err(anyhow!("Verdict model returned an unexpected logits shape"));
        }
        let logits_per_row = logits.len() / candidates.len();
        let logits_per_candidate = logits
            .chunks_exact(logits_per_row)
            .take(candidates.len())
            .collect::<Vec<_>>();
        if logits_per_candidate.len() != candidates.len() {
            return Err(anyhow!("Verdict model returned an unexpected logits shape"));
        }
        candidates
            .iter()
            .zip(logits_per_candidate)
            .map(|(candidate, row)| {
                let probability = calibrated_probabilities(row, 3, &self.calibrator)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow!("Verdict returned no caller-option probability"))?;
                Ok(Score {
                    candidate_id: candidate.id.clone(),
                    probability,
                })
            })
            .collect()
    }
}

#[async_trait]
impl DecisionBackend for LocalVerdictBackend {
    async fn health(&self) -> Result<BackendHealth> {
        Ok(BackendHealth {
            endpoint: "local://verdict-1.4".to_string(),
            reachable: true,
            model: "verdict-1.4".to_string(),
        })
    }

    async fn models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![ModelInfo {
            id: "verdict-1.4".to_string(),
        }])
    }

    async fn score_batch(&self, query: &str, candidates: &[Candidate]) -> Result<Vec<Score>> {
        self.score_sync(query, candidates)
    }
}
