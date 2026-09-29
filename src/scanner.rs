use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use walkdir::WalkDir;

use crate::Result;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_file_size: u64,
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub no_gitignore: bool,
    pub hidden: bool,
    pub follow: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_file_size: 1_048_576,
            excludes: Vec::new(),
            includes: Vec::new(),
            no_gitignore: false,
            hidden: false,
            follow: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusionReason {
    Ignored,
    DefaultDirectory,
    Hidden,
    Sensitive,
    Binary,
    InvalidUtf8,
    TooLarge,
    Symlink,
    Generated,
    Pattern,
    ReadError,
}

impl fmt::Display for ExclusionReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Ignored => "ignored",
            Self::DefaultDirectory => "default directory",
            Self::Hidden => "hidden",
            Self::Sensitive => "sensitive file",
            Self::Binary => "binary",
            Self::InvalidUtf8 => "invalid UTF-8",
            Self::TooLarge => "too large",
            Self::Symlink => "symlink",
            Self::Generated => "generated or minified",
            Self::Pattern => "pattern",
            Self::ReadError => "read error",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct Exclusion {
    pub path: PathBuf,
    pub reason: ExclusionReason,
}

#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub content: String,
    pub language: Option<String>,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub included: Vec<ScannedFile>,
    pub excluded: Vec<Exclusion>,
}

pub fn scan_repository(root: &Path, options: &ScanOptions) -> Result<ScanReport> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(anyhow::anyhow!("scan root is not a directory: {}", root.display()));
    }

    let ignore = build_ignore(&root, options)?;
    let excludes = build_globset(&options.excludes)?;
    let includes = build_globset(&options.includes)?;
    let mut report = ScanReport::default();

    for entry in WalkDir::new(&root).follow_links(options.follow) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let path = error.path().unwrap_or(&root).to_path_buf();
                report.excluded.push(Exclusion {
                    path,
                    reason: ExclusionReason::ReadError,
                });
                continue;
            }
        };
        let path = entry.path();
        if path == root {
            continue;
        }
        let relative = path.strip_prefix(&root)?.to_path_buf();
        let normalized = relative.to_string_lossy().replace('\\', "/");
        let file_type = entry.file_type();

        if file_type.is_symlink() && !options.follow {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Symlink,
            });
            continue;
        }
        if is_hidden(&relative) && !options.hidden {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Hidden,
            });
            continue;
        }
        if is_default_directory(&relative) && !includes.is_match(&normalized) {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::DefaultDirectory,
            });
            continue;
        }
        if file_type.is_dir() {
            continue;
        }
        if !options.no_gitignore
            && ignore
                .matched_path_or_any_parents(Path::new(&normalized), false)
                .is_ignore()
        {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Ignored,
            });
            continue;
        }
        if excludes.is_match(&normalized) && !includes.is_match(&normalized) {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Pattern,
            });
            continue;
        }
        if is_sensitive(&relative) {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Sensitive,
            });
            continue;
        }
        if is_generated(&relative) {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Generated,
            });
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                report.excluded.push(Exclusion {
                    path: relative,
                    reason: ExclusionReason::ReadError,
                });
                continue;
            }
        };
        if metadata.len() > options.max_file_size {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::TooLarge,
            });
            continue;
        }
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => {
                report.excluded.push(Exclusion {
                    path: relative,
                    reason: ExclusionReason::ReadError,
                });
                continue;
            }
        };
        if bytes.contains(&0) {
            report.excluded.push(Exclusion {
                path: relative,
                reason: ExclusionReason::Binary,
            });
            continue;
        }
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                report.excluded.push(Exclusion {
                    path: relative,
                    reason: ExclusionReason::InvalidUtf8,
                });
                continue;
            }
        };

        report.included.push(ScannedFile {
            path: path.to_path_buf(),
            relative_path: relative.clone(),
            language: language_for_path(&relative),
            content,
        });
    }

    report.included.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    report.excluded.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(report)
}

fn build_ignore(root: &Path, options: &ScanOptions) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    if !options.no_gitignore {
        for name in [".gitignore", ".ignore", ".ojgignore"] {
            let path = root.join(name);
            if path.is_file() {
                if let Some(error) = builder.add(path) {
                    return Err(error.into());
                }
            }
        }
    }
    Ok(builder.build()?)
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern)?);
    }
    Ok(builder.build()?)
}

fn is_hidden(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .starts_with('.')
    })
}

fn is_default_directory(path: &Path) -> bool {
    const DEFAULTS: &[&str] = &[
        ".git",
        "node_modules",
        "target",
        "dist",
        "build",
        ".next",
        ".nuxt",
        "coverage",
        "vendor",
        "venv",
        ".venv",
        "__pycache__",
        ".gradle",
        ".idea",
    ];
    path.components().any(|component| {
        DEFAULTS.contains(&component.as_os_str().to_string_lossy().as_ref())
    })
}

fn is_sensitive(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name == "id_rsa"
        || name == "id_ed25519"
        || name.starts_with("credentials.")
        || name.starts_with("secrets.")
}

fn is_generated(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || name.contains("generated")
}

fn language_for_path(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let language = match extension.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "java" => "java",
        "go" => "go",
        _ => return None,
    };
    Some(language.to_string())
}
