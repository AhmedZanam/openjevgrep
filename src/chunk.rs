use std::path::PathBuf;

use crate::scanner::ScannedFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkKind {
    Paragraph,
    Lines,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChunk {
    pub path: PathBuf,
    pub language: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub symbol: Option<String>,
    pub kind: ChunkKind,
    pub content: String,
}

pub fn fallback_chunks(
    file: &ScannedFile,
    max_lines: usize,
    context_lines: usize,
) -> Vec<SourceChunk> {
    let mut lines: Vec<&str> = file.content.split('\n').collect();
    if file.content.ends_with('\n') {
        lines.pop();
    }
    if lines.is_empty() || (lines.len() == 1 && lines[0].is_empty()) {
        return Vec::new();
    }

    let max_lines = max_lines.max(1);
    let overlap = context_lines.max(1).min(max_lines.saturating_sub(1));
    let mut chunks = Vec::new();
    let mut paragraph_start = None;

    for index in 0..=lines.len() {
        let is_boundary = index == lines.len() || lines[index].trim().is_empty();
        if is_boundary {
            if let Some(start) = paragraph_start.take() {
                let end = index;
                if end - start <= max_lines {
                    chunks.push(make_chunk(file, &lines, start, end, ChunkKind::Paragraph));
                } else {
                    add_windows(
                        &mut chunks,
                        file,
                        &lines,
                        start,
                        end,
                        max_lines,
                        overlap,
                    );
                }
            }
        } else if paragraph_start.is_none() {
            paragraph_start = Some(index);
        }
    }
    chunks
}

fn add_windows(
    chunks: &mut Vec<SourceChunk>,
    file: &ScannedFile,
    lines: &[&str],
    paragraph_start: usize,
    paragraph_end: usize,
    max_lines: usize,
    overlap: usize,
) {
    let step = max_lines.saturating_sub(overlap).max(1);
    let mut start = paragraph_start;
    loop {
        let end = (start + max_lines).min(paragraph_end);
        chunks.push(make_chunk(file, lines, start, end, ChunkKind::Lines));
        if end == paragraph_end {
            break;
        }
        start += step;
    }
}

fn make_chunk(
    file: &ScannedFile,
    lines: &[&str],
    start: usize,
    end: usize,
    kind: ChunkKind,
) -> SourceChunk {
    SourceChunk {
        path: file.relative_path.clone(),
        language: file.language.clone(),
        start_line: start + 1,
        end_line: end,
        symbol: None,
        kind,
        content: lines[start..end]
            .iter()
            .map(|line| line.trim_end_matches('\r'))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
