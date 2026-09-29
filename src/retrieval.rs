use crate::chunk::tree_sitter::deterministic_file_preview as chunk_preview;
use crate::chunk::SourceChunk;
use crate::scanner::ScannedFile;

pub fn uses_hierarchical_retrieval(chunk_count: usize, threshold: usize) -> bool {
    chunk_count >= threshold
}

pub fn deterministic_file_preview(
    file: &ScannedFile,
    chunks: &[SourceChunk],
    max_chars: usize,
) -> String {
    chunk_preview(file, chunks, max_chars)
}
