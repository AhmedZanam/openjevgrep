use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::search::{search, BackendMetadata, SearchOptions, SearchRequest};
use crate::backend::DecisionBackend;
use crate::output::render_json;
use crate::Result;

pub async fn handle_line(line: &str, backend: Arc<dyn DecisionBackend>) -> Result<String> {
    let request: RpcRequest = serde_json::from_str(line)?;
    let response = match request.method.as_str() {
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "result": {"tools": [tool_definition()]},
        }),
        "tools/call" => handle_tool(request.id, request.params, backend).await,
        _ => json!({
            "jsonrpc": "2.0",
            "id": request.id,
            "error": {"code": -32601, "message": "method not found"},
        }),
    };
    Ok(serde_json::to_string(&response)?)
}

pub async fn run_stdio<R, W>(
    mut reader: R,
    mut writer: W,
    backend: Arc<dyn DecisionBackend>,
) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(line.trim(), backend.clone()).await?;
        writer.write_all(response.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }
    Ok(())
}

async fn handle_tool(id: Value, params: Option<Value>, backend: Arc<dyn DecisionBackend>) -> Value {
    let Some(params) = params else {
        return error_response(id, -32602, "missing tool parameters");
    };
    let name = params.get("name").and_then(Value::as_str);
    if name != Some("semantic_search_code") {
        return error_response(id, -32602, "unknown tool");
    }
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let args: SearchArguments = match serde_json::from_value(arguments) {
        Ok(args) => args,
        Err(error) => return error_response(id, -32602, &error.to_string()),
    };
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    let mut options = SearchOptions::default();
    if let Some(threshold) = args.threshold {
        options.threshold = threshold;
    }
    if let Some(limit) = args.limit {
        options.limit = limit;
    }
    let model = args.model.unwrap_or_else(|| "configured".to_string());
    let request = SearchRequest {
        query: args.query,
        root: path,
        scopes: args.scopes.unwrap_or_default(),
        backend: BackendMetadata {
            endpoint: args.endpoint.unwrap_or_else(|| "configured".to_string()),
            model,
        },
        options,
    };
    match search(request, backend).await.and_then(|response| render_json(&response)) {
        Ok(text) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"content": [{"type": "text", "text": text}]},
        }),
        Err(error) => error_response(id, -32000, &error.to_string()),
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_definition() -> Value {
    json!({
        "name": "semantic_search_code",
        "description": "Find source code by describing behavior.",
        "inputSchema": {
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"},
                "path": {"type": "string"},
                "scopes": {"type": "array", "items": {"type": "string"}},
                "threshold": {"type": "number", "minimum": 0, "maximum": 1},
                "limit": {"type": "integer", "minimum": 1},
                "model": {"type": "string"},
                "endpoint": {"type": "string"}
            }
        }
    })
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: Value,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct SearchArguments {
    query: String,
    path: Option<PathBuf>,
    scopes: Option<Vec<PathBuf>>,
    threshold: Option<f64>,
    limit: Option<usize>,
    model: Option<String>,
    endpoint: Option<String>,
}
