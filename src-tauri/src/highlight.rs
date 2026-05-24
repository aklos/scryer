//! In-process syntax highlighting for the code inspector, using the same
//! tree-sitter grammars bundled for symbol resolution. Produces, per source
//! line, an ordered list of `{text, kind}` segments that concatenate back to
//! the line — so the frontend just colours each segment, no column math.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

thread_local! {
    // Compiled highlight configs are reused across calls — building one
    // recompiles a large tree-sitter query (~hundreds of ms), so we do it
    // once per language per worker thread.
    static CONFIG_CACHE: RefCell<HashMap<String, Option<HighlightConfiguration>>> =
        RefCell::new(HashMap::new());
}

#[derive(serde::Serialize, Clone)]
pub struct Segment {
    pub text: String,
    /// Coarse token class (empty = default text). Mapped to a colour in the UI.
    pub kind: String,
}

/// Capture names we ask tree-sitter for. Index into this list comes back on
/// each highlight; `class_for` collapses it to a coarse, themeable class.
const HL_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "escape",
    "function",
    "function.builtin",
    "function.method",
    "keyword",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "label",
    "module",
];

fn class_for(idx: usize) -> &'static str {
    let name = HL_NAMES.get(idx).copied().unwrap_or("");
    match name.split('.').next().unwrap_or("") {
        "comment" => "comment",
        "keyword" => "keyword",
        "string" | "escape" => "string",
        "number" => "number",
        "constant" => "constant",
        "function" | "constructor" => "function",
        "type" => "type",
        "property" | "attribute" => "property",
        "tag" => "tag",
        "operator" => "operator",
        "punctuation" => "punct",
        // variable / module / label / default → default text colour
        _ => "",
    }
}

fn build(language: Language, name: &str, query: &str) -> Option<HighlightConfiguration> {
    let mut cfg = HighlightConfiguration::new(language, name, query, "", "").ok()?;
    cfg.configure(HL_NAMES);
    Some(cfg)
}

fn config_for_ext(ext: &str) -> Option<HighlightConfiguration> {
    match ext {
        "rs" => build(tree_sitter_rust::LANGUAGE.into(), "rust", tree_sitter_rust::HIGHLIGHTS_QUERY),
        // TypeScript highlights build on the JavaScript ones.
        "ts" | "mts" | "cts" => {
            let q = format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            );
            build(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), "typescript", &q)
        }
        "tsx" => {
            let q = format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY
            );
            build(tree_sitter_typescript::LANGUAGE_TSX.into(), "tsx", &q)
        }
        "js" | "jsx" | "mjs" | "cjs" => build(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        ),
        "py" | "pyi" => {
            build(tree_sitter_python::LANGUAGE.into(), "python", tree_sitter_python::HIGHLIGHTS_QUERY)
        }
        "go" => build(tree_sitter_go::LANGUAGE.into(), "go", tree_sitter_go::HIGHLIGHTS_QUERY),
        "java" => {
            build(tree_sitter_java::LANGUAGE.into(), "java", tree_sitter_java::HIGHLIGHTS_QUERY)
        }
        "rb" => build(tree_sitter_ruby::LANGUAGE.into(), "ruby", tree_sitter_ruby::HIGHLIGHTS_QUERY),
        "c" | "h" => build(tree_sitter_c::LANGUAGE.into(), "c", tree_sitter_c::HIGHLIGHT_QUERY),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => {
            build(tree_sitter_cpp::LANGUAGE.into(), "cpp", tree_sitter_cpp::HIGHLIGHT_QUERY)
        }
        "cs" => build(
            tree_sitter_c_sharp::LANGUAGE.into(),
            "csharp",
            tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
        ),
        "php" => build(tree_sitter_php::LANGUAGE_PHP.into(), "php", tree_sitter_php::HIGHLIGHTS_QUERY),
        _ => None,
    }
}

/// Highlight the whole `source`, returning one segment-list per source line
/// (line N → index N-1). `None` for unsupported languages — the caller falls
/// back to plain (single default segment per line).
pub fn highlight_lines(path: &Path, source: &str) -> Option<Vec<Vec<Segment>>> {
    let ext = path.extension()?.to_str()?.to_string();
    CONFIG_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        let config = map
            .entry(ext.clone())
            .or_insert_with(|| config_for_ext(&ext))
            .as_ref()?;

        let mut highlighter = Highlighter::new();
        let events = highlighter
            .highlight(config, source.as_bytes(), None, |_| None)
            .ok()?;

        let mut lines: Vec<Vec<Segment>> = vec![Vec::new()];
        let mut stack: Vec<usize> = Vec::new();
        for event in events {
            match event.ok()? {
                HighlightEvent::HighlightStart(h) => stack.push(h.0),
                HighlightEvent::HighlightEnd => {
                    stack.pop();
                }
                HighlightEvent::Source { start, end } => {
                    let kind = stack.last().map(|i| class_for(*i)).unwrap_or("");
                    let text = source.get(start..end).unwrap_or("");
                    let mut first = true;
                    for piece in text.split('\n') {
                        if !first {
                            lines.push(Vec::new());
                        }
                        first = false;
                        if !piece.is_empty() {
                            lines.last_mut().unwrap().push(Segment {
                                text: piece.to_string(),
                                kind: kind.to_string(),
                            });
                        }
                    }
                }
            }
        }
        Some(lines)
    })
}
