use serde::Serialize;

use crate::search::{Coverage, SearchResponse, SearchResult, Timing};
use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Compact,
    Files,
}

pub fn render_text(response: &SearchResponse, mode: OutputMode) -> String {
    let mut output = String::new();
    for result in &response.results {
        match mode {
            OutputMode::Human => {
                output.push_str(&format!(
                    "{}  relevance={:.3}\n\n  {}-{}\n",
                    result.path, result.probability, result.start_line, result.end_line
                ));
                for line in result.source.lines() {
                    output.push_str("  ");
                    output.push_str(line);
                    output.push('\n');
                }
                output.push('\n');
            }
            OutputMode::Compact => {
                output.push_str(&format!(
                    "{}:{}-{}  {:.3}",
                    result.path, result.start_line, result.end_line, result.probability
                ));
                if let Some(symbol) = &result.symbol {
                    output.push_str("  ");
                    output.push_str(symbol);
                }
                output.push('\n');
            }
            OutputMode::Files => {
                output.push_str(&format!("{}  {:.3}\n", result.path, result.probability));
            }
        }
    }
    output
}

pub fn render_json(response: &SearchResponse) -> Result<String> {
    let root = response.root.to_string_lossy().into_owned();
    let value = JsonResponse {
        query: &response.query,
        root: &root,
        backend: JsonBackend {
            backend_type: "openjev",
            endpoint: &response.backend.endpoint,
            model: &response.backend.model,
        },
        results: &response.results,
        coverage: &response.coverage,
        timing: &response.timing,
    };
    Ok(serde_json::to_string_pretty(&value)?)
}

#[derive(Serialize)]
struct JsonResponse<'a> {
    query: &'a str,
    root: &'a str,
    backend: JsonBackend<'a>,
    results: &'a [SearchResult],
    coverage: &'a Coverage,
    timing: &'a Timing,
}

#[derive(Serialize)]
struct JsonBackend<'a> {
    #[serde(rename = "type")]
    backend_type: &'static str,
    endpoint: &'a str,
    model: &'a str,
}
