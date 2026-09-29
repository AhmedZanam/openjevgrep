use std::path::PathBuf;

use ojg_core::chunk::language::language_for_path;
use ojg_core::chunk::tree_sitter::{source_chunks, ChunkOptions};
use ojg_core::retrieval::{deterministic_file_preview, uses_hierarchical_retrieval};
use ojg_core::scanner::ScannedFile;

#[test]
fn preview_contains_path_language_symbols_and_signatures() {
    let file = ScannedFile {
        path: PathBuf::from("src/auth.rs"),
        relative_path: PathBuf::from("src/auth.rs"),
        language: language_for_path(PathBuf::from("src/auth.rs").as_path())
            .map(|kind| kind.as_str().to_string()),
        content: "use crate::token;\nfn validate(token: &str) -> bool {\n    true\n}\n".to_string(),
    };
    let chunks = source_chunks(&file, &ChunkOptions::default()).expect("chunks");
    let preview = deterministic_file_preview(&file, &chunks, 300);

    assert!(preview.contains("path: src/auth.rs"));
    assert!(preview.contains("language: rust"));
    assert!(preview.contains("validate"));
    assert!(preview.contains("fn validate"));
}

#[test]
fn hierarchical_retrieval_activates_at_configured_chunk_threshold() {
    assert!(!uses_hierarchical_retrieval(149, 150));
    assert!(uses_hierarchical_retrieval(150, 150));
}
