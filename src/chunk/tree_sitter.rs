use tree_sitter::{Language, Node, Parser};

use super::language::{language_for_path, LanguageKind};
use super::{fallback_chunks, ChunkKind, SourceChunk};
use crate::scanner::ScannedFile;
use crate::Result;

#[derive(Debug, Clone)]
pub struct ChunkOptions {
    pub max_lines: usize,
    pub context_lines: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_lines: 160,
            context_lines: 2,
        }
    }
}

pub fn source_chunks(file: &ScannedFile, options: &ChunkOptions) -> Result<Vec<SourceChunk>> {
    let Some(language_kind) = language_for_path(&file.relative_path) else {
        return Ok(fallback_chunks(
            file,
            options.max_lines,
            options.context_lines,
        ));
    };
    let mut parser = Parser::new();
    parser.set_language(language(language_kind))?;
    let Some(tree) = parser.parse(&file.content, None) else {
        return Ok(fallback_chunks(
            file,
            options.max_lines,
            options.context_lines,
        ));
    };
    let mut nodes = Vec::new();
    collect_relevant_nodes(tree.root_node(), &mut nodes);
    if nodes.is_empty() {
        return Ok(fallback_chunks(
            file,
            options.max_lines,
            options.context_lines,
        ));
    }

    let mut chunks = Vec::new();
    for node in nodes {
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;
        let chunk = make_ast_chunk(file, node, language_kind, start_line, end_line);
        if end_line - start_line + 1 > options.max_lines.max(1) {
            chunks.extend(split_for_backend(&chunk, options.max_lines));
        } else {
            chunks.push(chunk);
        }
    }
    chunks.sort_by(|left, right| {
        left.start_line
            .cmp(&right.start_line)
            .then_with(|| left.end_line.cmp(&right.end_line))
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    Ok(chunks)
}

pub fn deterministic_file_preview(
    file: &ScannedFile,
    chunks: &[SourceChunk],
    max_chars: usize,
) -> String {
    let mut preview = format!(
        "path: {}\nlanguage: {}\n",
        file.relative_path.to_string_lossy().replace('\\', "/"),
        file.language.as_deref().unwrap_or("text")
    );
    for line in file
        .content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
    {
        preview.push_str(line.trim());
        preview.push('\n');
    }
    for chunk in chunks.iter().take(32) {
        if let Some(symbol) = &chunk.symbol {
            preview.push_str("symbol: ");
            preview.push_str(symbol);
            preview.push('\n');
        }
    }
    preview.chars().take(max_chars).collect()
}

pub fn split_for_backend(chunk: &SourceChunk, max_lines: usize) -> Vec<SourceChunk> {
    let lines: Vec<&str> = chunk.content.lines().collect();
    if lines.is_empty() {
        return vec![chunk.clone()];
    }
    let max_lines = max_lines.max(1);
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < lines.len() {
        let end = (offset + max_lines).min(lines.len());
        result.push(SourceChunk {
            path: chunk.path.clone(),
            language: chunk.language.clone(),
            start_line: chunk.start_line + offset,
            end_line: chunk.start_line + end - 1,
            symbol: chunk.symbol.clone(),
            kind: chunk.kind.clone(),
            content: lines[offset..end].join("\n"),
        });
        offset = end;
    }
    result
}

fn collect_relevant_nodes<'tree>(node: Node<'tree>, output: &mut Vec<Node<'tree>>) {
    if is_relevant_node(node.kind()) {
        output.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_relevant_nodes(child, output);
    }
}

fn is_relevant_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "constructor_declaration"
            | "class_definition"
            | "class_declaration"
            | "struct_item"
            | "impl_item"
            | "trait_item"
            | "interface_declaration"
            | "type_declaration"
            | "type_alias_declaration"
            | "enum_item"
            | "enum_declaration"
            | "mod_item"
            | "record_declaration"
    )
}

fn make_ast_chunk(
    file: &ScannedFile,
    node: Node<'_>,
    language: LanguageKind,
    start_line: usize,
    end_line: usize,
) -> SourceChunk {
    let lines: Vec<&str> = file.content.lines().collect();
    let source = lines
        .get(start_line.saturating_sub(1)..end_line.min(lines.len()))
        .unwrap_or(&[])
        .join("\n");
    SourceChunk {
        path: file.relative_path.clone(),
        language: Some(language.as_str().to_string()),
        start_line,
        end_line,
        symbol: node_symbol(node, file.content.as_bytes()),
        kind: node_kind(node.kind()),
        content: source,
    }
}

fn node_symbol(node: Node<'_>, source: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return name.utf8_text(source).ok().map(ToString::to_string);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "identifier" | "type_identifier" | "property_identifier" | "field_identifier"
        ) {
            return child.utf8_text(source).ok().map(ToString::to_string);
        }
    }
    None
}

fn node_kind(kind: &str) -> ChunkKind {
    if kind.contains("function") {
        ChunkKind::Function
    } else if kind.contains("method") || kind.contains("constructor") {
        ChunkKind::Method
    } else if kind.contains("class") || kind.contains("struct") || kind.contains("interface") {
        ChunkKind::Class
    } else {
        ChunkKind::Declaration
    }
}

fn language(kind: LanguageKind) -> Language {
    match kind {
        LanguageKind::Rust => tree_sitter_rust::language(),
        LanguageKind::Python => tree_sitter_python::language(),
        LanguageKind::JavaScript => tree_sitter_javascript::language(),
        LanguageKind::TypeScript => tree_sitter_typescript::language_typescript(),
        LanguageKind::Tsx => tree_sitter_typescript::language_tsx(),
        LanguageKind::Java => tree_sitter_java::language(),
        LanguageKind::Go => tree_sitter_go::language(),
    }
}
