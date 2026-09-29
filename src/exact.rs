use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::scanner::{scan_repository, ScanOptions};
use crate::Result;

#[derive(Debug, Clone)]
pub struct ExactOptions {
    pub use_rg: bool,
    pub hidden: bool,
    pub follow: bool,
    pub no_ignore: bool,
    pub max_file_size: u64,
}

impl Default for ExactOptions {
    fn default() -> Self {
        Self {
            use_rg: true,
            hidden: false,
            follow: false,
            no_ignore: false,
            max_file_size: 1_048_576,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactMatch {
    pub path: String,
    pub line: usize,
    pub text: String,
}

pub fn exact_search(query: &str, roots: &[PathBuf], options: &ExactOptions) -> Result<Vec<ExactMatch>> {
    if query.is_empty() {
        return Err(anyhow::anyhow!("exact query must not be empty"));
    }
    if options.use_rg && roots.len() == 1 && rg_available() {
        return run_rg(query, &roots[0], options);
    }
    run_fallback(query, roots, options)
}

fn rg_available() -> bool {
    Command::new("rg")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_rg(query: &str, root: &Path, options: &ExactOptions) -> Result<Vec<ExactMatch>> {
    let current_dir = if root.is_dir() {
        root.to_path_buf()
    } else {
        root.parent().unwrap_or(root).to_path_buf()
    };
    let target = if root.is_dir() {
        PathBuf::from(".")
    } else {
        root.file_name()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("exact search path has no filename"))?
    };
    let mut command = Command::new("rg");
    command
        .current_dir(current_dir)
        .args(["--json", "--fixed-strings", "--color", "never"]);
    if options.hidden {
        command.arg("--hidden");
    }
    if options.follow {
        command.arg("--follow");
    }
    if options.no_ignore {
        command.arg("--no-ignore");
    }
    let output = command.arg("--").arg(query).arg(target).output()?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(anyhow::anyhow!(
            "rg failed with status {}",
            output.status
        ));
    }
    let mut matches = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let event: RgEvent = match serde_json::from_str(line) {
            Ok(event) => event,
            Err(_) => continue,
        };
        if event.event_type != "match" {
            continue;
        }
        if let Some(data) = event.data {
            matches.push(ExactMatch {
                path: data.path.text.replace('\\', "/"),
                line: data.line_number,
                text: data.lines.text.trim_end_matches(['\r', '\n']).to_string(),
            });
        }
    }
    Ok(matches)
}

fn run_fallback(query: &str, roots: &[PathBuf], options: &ExactOptions) -> Result<Vec<ExactMatch>> {
    let mut matches = Vec::new();
    for root in roots {
        let report = scan_repository(
            root,
            &ScanOptions {
                max_file_size: options.max_file_size,
                hidden: options.hidden,
                follow: options.follow,
                no_gitignore: options.no_ignore,
                ..ScanOptions::default()
            },
        )?;
        for file in report.included {
            for (line, text) in file.content.lines().enumerate() {
                if text.contains(query) {
                    matches.push(ExactMatch {
                        path: file.relative_path.to_string_lossy().replace('\\', "/"),
                        line: line + 1,
                        text: text.to_string(),
                    });
                }
            }
        }
    }
    matches.sort_by(|left, right| left.path.cmp(&right.path).then(left.line.cmp(&right.line)));
    Ok(matches)
}

#[derive(Debug, Deserialize)]
struct RgEvent {
    #[serde(rename = "type")]
    event_type: String,
    data: Option<RgData>,
}

#[derive(Debug, Deserialize)]
struct RgData {
    path: RgPath,
    lines: RgLines,
    line_number: usize,
}

#[derive(Debug, Deserialize)]
struct RgPath {
    text: String,
}

#[derive(Debug, Deserialize)]
struct RgLines {
    text: String,
}
