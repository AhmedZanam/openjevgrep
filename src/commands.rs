use std::path::{Path, PathBuf};

use crate::backend::{BackendHealth, DecisionBackend, ModelInfo};
use crate::chunk::tree_sitter::{source_chunks, ChunkOptions};
use crate::scanner::{scan_repository, Exclusion, ScanOptions};
use crate::Result;

#[derive(Debug, Clone)]
pub struct InspectOptions {
    pub batch_size: usize,
    pub max_file_size: u64,
    pub max_chunk_lines: usize,
    pub context_lines: usize,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            batch_size: 16,
            max_file_size: 1_048_576,
            max_chunk_lines: 160,
            context_lines: 2,
        }
    }
}

#[derive(Debug)]
pub struct InspectReport {
    pub included_files: Vec<PathBuf>,
    pub excluded_files: Vec<Exclusion>,
    pub files_included: usize,
    pub chunks_found: usize,
    pub estimated_requests: usize,
    pub estimated_input_bytes: usize,
}

pub fn inspect_repository(root: &Path, options: &InspectOptions) -> Result<InspectReport> {
    let report = scan_repository(
        root,
        &ScanOptions {
            max_file_size: options.max_file_size,
            ..ScanOptions::default()
        },
    )?;
    let mut chunks_found = 0;
    let mut estimated_input_bytes = 0;
    let mut included_files = Vec::new();
    for file in &report.included {
        let chunks = source_chunks(
            file,
            &ChunkOptions {
                max_lines: options.max_chunk_lines,
                context_lines: options.context_lines,
            },
        )?;
        chunks_found += chunks.len();
        estimated_input_bytes += chunks
            .iter()
            .map(|chunk| chunk.content.len())
            .sum::<usize>();
        included_files.push(file.relative_path.clone());
    }
    Ok(InspectReport {
        included_files,
        excluded_files: report.excluded,
        files_included: report.included.len(),
        chunks_found,
        estimated_requests: chunks_found.div_ceil(options.batch_size.max(1)),
        estimated_input_bytes,
    })
}

pub async fn check_backend(
    backend: &dyn DecisionBackend,
    selected_model: &str,
) -> Result<(BackendHealth, bool, Vec<ModelInfo>)> {
    let health = backend.health().await?;
    let models = backend.models().await?;
    let selected = models.iter().any(|model| model.id == selected_model);
    Ok((health, selected, models))
}
