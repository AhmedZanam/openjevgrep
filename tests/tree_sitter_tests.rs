use std::path::PathBuf;

use ojg_core::chunk::language::{language_for_path, LanguageKind};
use ojg_core::chunk::tree_sitter::{source_chunks, ChunkOptions};
use ojg_core::chunk::{ChunkKind, SourceChunk};
use ojg_core::scanner::ScannedFile;

fn file(path: &str, content: &str) -> ScannedFile {
    ScannedFile {
        path: PathBuf::from(path),
        relative_path: PathBuf::from(path),
        content: content.to_string(),
        language: language_for_path(PathBuf::from(path).as_path()).map(|kind| kind.as_str().to_string()),
    }
}

#[test]
fn detects_all_supported_languages() {
    let cases = [
        ("main.rs", LanguageKind::Rust),
        ("main.py", LanguageKind::Python),
        ("main.js", LanguageKind::JavaScript),
        ("main.ts", LanguageKind::TypeScript),
        ("main.tsx", LanguageKind::Tsx),
        ("Main.java", LanguageKind::Java),
        ("main.go", LanguageKind::Go),
    ];

    for (path, expected) in cases {
        assert_eq!(language_for_path(PathBuf::from(path).as_path()), Some(expected));
    }
}

#[test]
fn extracts_rust_function_with_symbol_and_exact_lines() {
    let chunks = source_chunks(
        &file("src/auth.rs", "fn validate(token: &str) -> bool {\n    !token.is_empty()\n}\n"),
        &ChunkOptions::default(),
    )
    .expect("rust chunks");

    let function = chunks
        .iter()
        .find(|chunk| chunk.kind == ChunkKind::Function)
        .expect("function chunk");
    assert_eq!(function.symbol.as_deref(), Some("validate"));
    assert_eq!((function.start_line, function.end_line), (1, 3));
    assert_eq!(function.content, "fn validate(token: &str) -> bool {\n    !token.is_empty()\n}");
}

#[test]
fn extracts_representative_declarations_for_each_grammar() {
    let cases = [
        ("main.py", "class Auth:\n    def validate(self):\n        return True\n", ChunkKind::Class),
        ("main.js", "function validate(token) {\n  return Boolean(token);\n}\n", ChunkKind::Function),
        ("main.ts", "export function validate(token: string): boolean {\n  return !!token;\n}\n", ChunkKind::Function),
        ("main.tsx", "export function Auth() {\n  return <div />;\n}\n", ChunkKind::Function),
        ("Main.java", "class Auth {\n  boolean validate(String token) { return true; }\n}\n", ChunkKind::Class),
        ("main.go", "func Validate(token string) bool {\n  return token != \"\"\n}\n", ChunkKind::Function),
    ];

    for (path, content, expected_kind) in cases {
        let chunks = source_chunks(&file(path, content), &ChunkOptions::default()).expect("chunks");
        assert!(chunks.iter().any(|chunk| chunk.kind == expected_kind), "{path}");
    }
}

#[test]
fn oversized_chunk_preserves_source_lines() {
    let content = "fn validate() {\nline 2\nline 3\nline 4\nline 5\n}\n";
    let options = ChunkOptions {
        max_lines: 2,
        context_lines: 1,
    };
    let chunks = source_chunks(&file("src/auth.rs", content), &options).expect("chunks");

    assert!(chunks.len() >= 3);
    assert_eq!(chunks.first().expect("first").start_line, 1);
    assert_eq!(chunks.last().expect("last").end_line, 6);
    assert!(chunks.iter().all(|chunk| chunk.start_line <= chunk.end_line));
}

#[test]
fn split_for_backend_keeps_line_ranges_and_source() {
    let chunk = SourceChunk {
        path: "main.txt".into(),
        language: None,
        start_line: 10,
        end_line: 13,
        symbol: None,
        kind: ChunkKind::Declaration,
        content: "a\nb\nc\nd".to_string(),
    };
    let chunks = ojg_core::chunk::tree_sitter::split_for_backend(&chunk, 2);
    assert_eq!(chunks.len(), 2);
    assert_eq!((chunks[0].start_line, chunks[0].end_line), (10, 11));
    assert_eq!((chunks[1].start_line, chunks[1].end_line), (12, 13));
    assert_eq!(chunks[1].content, "c\nd");
}
