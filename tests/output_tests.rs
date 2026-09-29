use ojg_core::output::{render_json, render_text, OutputMode};
use ojg_core::search::{
    BackendMetadata, Coverage, SearchResponse, SearchResult, Timing,
};

fn response() -> SearchResponse {
    SearchResponse {
        query: "where is auth checked?".to_string(),
        root: "/repo".into(),
        backend: BackendMetadata {
            endpoint: "http://127.0.0.1:8080".to_string(),
            model: "verdict-1.4".to_string(),
        },
        results: vec![SearchResult {
            path: "src/auth.rs".to_string(),
            start_line: 42,
            end_line: 44,
            symbol: Some("validate".to_string()),
            kind: "function".to_string(),
            probability: 0.972,
            source: "fn validate() {}".to_string(),
        }],
        coverage: Coverage {
            files_scanned: 3,
            chunks_found: 5,
            chunks_evaluated: 2,
            partial: false,
            partial_reason: None,
        },
        timing: Timing {
            scan_ms: 3,
            evaluation_ms: 7,
            total_ms: 10,
        },
    }
}

#[test]
fn renders_human_compact_and_files_only_modes() {
    let response = response();
    let human = render_text(&response, OutputMode::Human);
    assert!(human.contains("src/auth.rs  relevance=0.972"));
    assert!(human.contains("42-44"));
    assert!(human.contains("fn validate() {}"));

    let compact = render_text(&response, OutputMode::Compact);
    assert_eq!(compact, "src/auth.rs:42-44  0.972  validate\n");

    let files = render_text(&response, OutputMode::Files);
    assert_eq!(files, "src/auth.rs  0.972\n");
}

#[test]
fn renders_versioned_json_with_coverage_and_timing() {
    let json = render_json(&response()).expect("json");
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid json");
    assert_eq!(value["query"], "where is auth checked?");
    assert_eq!(value["backend"]["type"], "openjev");
    assert_eq!(value["results"][0]["start_line"], 42);
    assert_eq!(value["coverage"]["chunks_evaluated"], 2);
    assert_eq!(value["timing"]["total_ms"], 10);
}
