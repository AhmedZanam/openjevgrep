use std::path::PathBuf;

use ojg_core::chunk::{fallback_chunks, ChunkKind};
use ojg_core::scanner::ScannedFile;

fn file(content: &str) -> ScannedFile {
    ScannedFile {
        path: PathBuf::from("src/example.txt"),
        relative_path: PathBuf::from("src/example.txt"),
        content: content.to_string(),
        language: None,
    }
}

#[test]
fn creates_paragraph_chunks_with_exact_line_ranges() {
    let chunks = fallback_chunks(&file("first line\nsecond line\n\nthird line\n"), 10, 2);

    assert_eq!(chunks.len(), 2);
    assert_eq!((chunks[0].start_line, chunks[0].end_line), (1, 2));
    assert_eq!(chunks[0].kind, ChunkKind::Paragraph);
    assert_eq!(chunks[1].start_line, 4);
    assert_eq!(chunks[1].content, "third line");
}

#[test]
fn overlaps_line_windows_and_covers_oversized_text() {
    let content = (1..=9)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let chunks = fallback_chunks(&file(&content), 4, 1);

    assert!(chunks.len() >= 3);
    assert_eq!(chunks[0].kind, ChunkKind::Lines);
    assert_eq!(chunks[0].start_line, 1);
    assert_eq!(chunks[0].end_line, 4);
    assert!(chunks
        .windows(2)
        .all(|pair| pair[0].end_line >= pair[1].start_line));
    assert_eq!(chunks.last().expect("last chunk").end_line, 9);
}
