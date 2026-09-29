use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageKind {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Java,
    Go,
}

impl LanguageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Java => "java",
            Self::Go => "go",
        }
    }
}

pub fn language_for_path(path: &Path) -> Option<LanguageKind> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "rs" => Some(LanguageKind::Rust),
        "py" => Some(LanguageKind::Python),
        "js" | "jsx" | "mjs" | "cjs" => Some(LanguageKind::JavaScript),
        "ts" => Some(LanguageKind::TypeScript),
        "tsx" => Some(LanguageKind::Tsx),
        "java" => Some(LanguageKind::Java),
        "go" => Some(LanguageKind::Go),
        _ => None,
    }
}
