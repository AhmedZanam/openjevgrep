use std::sync::Arc;

use async_trait::async_trait;
use ojg_core::backend::{BackendHealth, Candidate, DecisionBackend, ModelInfo, Score};
use ojg_core::mcp::{handle_line, run_stdio};
use tempfile::tempdir;
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

struct FakeBackend;

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
        Ok(Vec::new())
    }

    async fn score_batch(&self, _query: &str, candidates: &[Candidate]) -> ojg_core::Result<Vec<Score>> {
        Ok(candidates
            .iter()
            .map(|candidate| Score {
                candidate_id: candidate.id.clone(),
                probability: 0.9,
            })
            .collect())
    }
}

#[tokio::test]
async fn handles_semantic_search_code_tool_call() {
    let root = tempdir().expect("root");
    std::fs::write(root.path().join("auth.rs"), "fn validate() {}\n").expect("source");
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "semantic_search_code",
            "arguments": {
                "query": "where is auth checked?",
                "path": root.path()
            }
        }
    });

    let response = handle_line(&request.to_string(), Arc::new(FakeBackend))
        .await
        .expect("response");
    let value: serde_json::Value = serde_json::from_str(&response).expect("json");
    assert_eq!(value["result"]["content"][0]["type"], "text");
    assert!(value["result"]["content"][0]["text"]
        .as_str()
        .expect("text")
        .contains("auth.rs"));
}

#[tokio::test]
async fn mcp_keeps_diagnostics_off_stdout() {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let input = format!("{}\n", request);
    let (client, server) = duplex(4096);
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let (server_read, server_write) = tokio::io::split(server);
    let backend = Arc::new(FakeBackend);
    let task = tokio::spawn(async move {
        run_stdio(
            tokio::io::BufReader::new(server_read),
            tokio::io::BufWriter::new(server_write),
            backend,
        )
        .await
    });
    client_write.write_all(input.as_bytes()).await.expect("input");
    client_write.shutdown().await.expect("shutdown");

    let mut output = Vec::new();
    client_read.read_to_end(&mut output).await.expect("output");
    task.await.expect("task").expect("stdio");
    let response: serde_json::Value =
        serde_json::from_str(std::str::from_utf8(&output).expect("utf8").trim())
            .expect("json-rpc stdout");
    assert_eq!(response["result"]["tools"][0]["name"], "semantic_search_code");
}
