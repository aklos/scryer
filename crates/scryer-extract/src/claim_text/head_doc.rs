//! The module doc of a language with no doc-comment form for a whole file:
//! the comment at the head of the file, before any code (C#, Java, Kotlin,
//! Go, C, C++, PHP, Ruby), and Clojure's namespace docstring. Read here for the
//! lifter and rewritten here for the editors, so both agree on where it is.
//!
//! A head comment is the file's doc unless it is a licence, which is skipped,
//! or it sits right on a declaration (no blank line between), where it is
//! that declaration's doc. Right on `package`, `using`, `import` and the like
//! it is still the file's: Go's package comment sits on its package clause.

use tree_sitter::{Node, Parser};

/// How a doc is written, so an edit keeps the form the file uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Style {
    /// Line comments with this marker as written (`//`, `///`, `#`).
    Line(String),
    /// A `/* … */` or `/** … */` block.
    Block,
    /// A Clojure `ns` docstring.
    Docstring,
}

/// A file's module doc: its byte span and its text, one entry per line.
#[derive(Debug, Clone)]
pub(crate) struct HeadDoc {
    pub start: usize,
    pub end: usize,
    pub lines: Vec<String>,
    pub style: Style,
}

/// Where a new doc goes when a file has none: the byte offset, and the
/// text to put either side of it.
pub(crate) struct Insert {
    pub at: usize,
    pub before: &'static str,
    pub after: &'static str,
}

/// The line-comment marker of a language whose module doc is its head
/// comment.
pub(crate) fn line_marker(ext: &str) -> Option<&'static str> {
    match ext {
        "cs" | "java" | "kt" | "kts" | "go" | "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "php" => Some("//"),
        "rb" => Some("#"),
        _ => None,
    }
}

pub(crate) fn is_clojure(ext: &str) -> bool {
    matches!(ext, "clj" | "cljs" | "cljc" | "cljr")
}

/// The module doc of a file in one of these languages, if it has one.
pub(crate) fn head_doc(ext: &str, source: &str) -> Option<HeadDoc> {
    if is_clojure(ext) {
        return clojure_doc(source);
    }
    let marker = line_marker(ext)?;
    let head = scan(marker, source);
    let pick = head.blocks.iter().position(|b| !is_licence(&b.raw))?;
    let block = &head.blocks[pick];
    let last = pick + 1 == head.blocks.len();
    if last && head.attached && !head.next_code.is_some_and(is_header_line) {
        return None;
    }
    let mut lines = clean(&block.raw, &block.style, ext);
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    (!lines.is_empty()).then(|| HeadDoc { start: block.start, end: block.end, lines, style: block.style.clone() })
}

/// Where a new module doc goes in a file that has none; none in a Clojure
/// file with no namespace form to hold a docstring.
pub(crate) fn insert_point(ext: &str, source: &str) -> Option<Insert> {
    if is_clojure(ext) {
        return clojure_insert(source);
    }
    let marker = line_marker(ext).unwrap_or("//");
    let head = scan(marker, source);
    // Go's package comment sits on its package clause.
    if ext == "go" {
        if let Some(at) = head.package_at {
            return Some(Insert { at, before: "", after: "" });
        }
    }
    Some(match head.blocks.first() {
        Some(b) if is_licence(&b.raw) => Insert { at: b.end, before: "\n", after: "\n" },
        _ => Insert { at: head.preamble_end, before: "", after: "\n" },
    })
}

/// The doc's text in its style, ready to splice over its span.
pub(crate) fn render(lines: &[String], style: &Style) -> String {
    match style {
        Style::Line(marker) => lines
            .iter()
            .map(|l| if l.is_empty() { format!("{marker}\n") } else { format!("{marker} {l}\n") })
            .collect(),
        Style::Block => {
            let body: String = lines
                .iter()
                .map(|l| if l.is_empty() { " *\n".to_string() } else { format!(" * {}\n", l.replace("*/", "* /")) })
                .collect();
            format!("/**\n{body} */\n")
        }
        Style::Docstring => {
            // Continuation lines indent under the `ns`; blank ones stay bare.
            let body: Vec<String> = lines
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let l = l.replace('\\', "\\\\").replace('"', "\\\"");
                    if i == 0 || l.is_empty() { l } else { format!("  {l}") }
                })
                .collect();
            format!("\"{}\"", body.join("\n"))
        }
    }
}

/// The style a new doc is written in.
pub(crate) fn new_style(ext: &str) -> Style {
    if is_clojure(ext) {
        return Style::Docstring;
    }
    Style::Line(line_marker(ext).unwrap_or("//").to_string())
}

struct Block {
    start: usize,
    end: usize,
    raw: Vec<String>,
    style: Style,
}

struct Head<'s> {
    /// Where the lines a file must open with end (shebang, `<?php`, Ruby's
    /// magic comments).
    preamble_end: usize,
    blocks: Vec<Block>,
    /// The last block runs straight into code, no blank line between.
    attached: bool,
    next_code: Option<&'s str>,
    /// Go's `package` clause, where its doc goes.
    package_at: Option<usize>,
}

fn scan<'s>(marker: &str, source: &'s str) -> Head<'s> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut at = 0;
    for l in source.split_inclusive('\n') {
        lines.push((at, l));
        at += l.len();
    }
    let end_of = |i: usize| lines.get(i).map_or(source.len(), |(s, _)| *s);
    let mut i = 0;
    while let Some((_, l)) = lines.get(i) {
        let t = l.trim();
        let magic = marker == "#"
            && ["# frozen_string_literal", "# encoding", "# coding", "# -*-"].iter().any(|m| t.starts_with(m));
        if (i == 0 && t.starts_with("#!")) || t.starts_with("<?php") || magic {
            i += 1;
        } else {
            break;
        }
    }
    let preamble_end = end_of(i);
    let mut blocks = Vec::new();
    let mut attached = false;
    let mut next_code = None;
    let mut package_at = None;
    while i < lines.len() {
        let t = lines[i].1.trim();
        if t.is_empty() {
            attached = false;
            i += 1;
            continue;
        }
        let start = lines[i].0;
        if t.starts_with("/*") {
            let mut raw = Vec::new();
            while i < lines.len() {
                let l = lines[i].1.trim_end_matches(['\n', '\r']);
                raw.push(l.to_string());
                i += 1;
                if l.contains("*/") {
                    break;
                }
            }
            blocks.push(Block { start, end: end_of(i), raw, style: Style::Block });
            attached = true;
            continue;
        }
        if t.starts_with(marker) && !(marker == "#" && t.starts_with("#{")) {
            let written = if marker == "//" {
                let slashes = t.chars().take_while(|c| *c == '/').count();
                let bang = t[slashes..].starts_with('!');
                format!("{}{}", "/".repeat(slashes), if bang { "!" } else { "" })
            } else {
                marker.to_string()
            };
            let mut raw = Vec::new();
            while i < lines.len() && lines[i].1.trim().starts_with(marker) {
                raw.push(lines[i].1.trim_end_matches(['\n', '\r']).to_string());
                i += 1;
            }
            blocks.push(Block { start, end: end_of(i), raw, style: Style::Line(written) });
            attached = true;
            continue;
        }
        next_code = Some(t);
        if t.starts_with("package ") {
            package_at = Some(start);
        }
        break;
    }
    if next_code.is_none() {
        attached = false;
    }
    Head { preamble_end, blocks, attached, next_code, package_at }
}

/// A line that opens a file rather than declares something: a doc right on
/// it still documents the file.
fn is_header_line(line: &str) -> bool {
    const HEADERS: &[&str] = &[
        "package ", "import ", "using ", "namespace ", "require", "use ", "declare(", "#include", "#pragma", "#ifndef",
        "#define", "#if", "[assembly:", "global using ", "module ",
    ];
    HEADERS.iter().any(|h| line.starts_with(h))
}

fn is_licence(raw: &[String]) -> bool {
    let text = raw.join(" ").to_lowercase();
    ["copyright", "spdx-license", "licensed under", "all rights reserved", "license:"].iter().any(|k| text.contains(k))
}

/// A comment block's lines as prose: markers gone, C#'s XML tags dropped.
fn clean(raw: &[String], style: &Style, ext: &str) -> Vec<String> {
    let last = raw.len().saturating_sub(1);
    let lines: Vec<String> = raw
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let l = l.trim_start();
            let l = match style {
                Style::Line(marker) => l.strip_prefix(marker.as_str()).unwrap_or(l),
                Style::Block => {
                    let l = if i == 0 { l.trim_start_matches("/**").trim_start_matches("/*") } else { l };
                    let l = if i == last { l.trim_end().trim_end_matches("*/") } else { l };
                    let l = l.trim_start();
                    if i == 0 { l } else { l.strip_prefix('*').unwrap_or(l) }
                }
                Style::Docstring => l,
            };
            let l = l.strip_prefix(' ').unwrap_or(l).trim_end();
            if ext == "cs" { super::claims::strip_xml_tags(l).trim().to_string() } else { l.to_string() }
        })
        .collect();
    lines
}

/// The `(ns name "doc" …)` form's name and docstring nodes.
fn clojure_ns(root: Node) -> Option<(Node, Option<Node>)> {
    let mut cursor = root.walk();
    let ns = root.named_children(&mut cursor).find(|n| n.kind() == "list_lit")?;
    let mut c = ns.walk();
    let values: Vec<Node> = ns.named_children(&mut c).filter(|n| !matches!(n.kind(), "comment" | "meta_lit")).collect();
    let name = values.get(1)?;
    Some((*name, values.get(2).filter(|n| n.kind() == "str_lit").copied()))
}

fn parse_clojure(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_clojure::LANGUAGE.into()).ok()?;
    parser.parse(source, None)
}

fn is_ns_form(source: &str, root: Node) -> bool {
    let mut cursor = root.walk();
    let Some(form) = root.named_children(&mut cursor).find(|n| n.kind() == "list_lit") else { return false };
    let mut c = form.walk();
    let head = form.named_children(&mut c).find(|n| !matches!(n.kind(), "comment" | "meta_lit"));
    head.is_some_and(|h| h.kind() == "sym_lit" && &source[h.start_byte()..h.end_byte()] == "ns")
}

fn clojure_doc(source: &str) -> Option<HeadDoc> {
    let tree = parse_clojure(source)?;
    if !is_ns_form(source, tree.root_node()) {
        return None;
    }
    let (_, doc) = clojure_ns(tree.root_node())?;
    let doc = doc?;
    let raw = &source[doc.start_byte()..doc.end_byte()];
    let inner = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(raw);
    let unescaped = inner.replace("\\\"", "\"").replace("\\\\", "\\");
    let lines: Vec<String> = unescaped.lines().map(|l| l.trim().to_string()).collect();
    Some(HeadDoc { start: doc.start_byte(), end: doc.end_byte(), lines, style: Style::Docstring })
}

fn clojure_insert(source: &str) -> Option<Insert> {
    let tree = parse_clojure(source).filter(|t| is_ns_form(source, t.root_node()))?;
    let (name, _) = clojure_ns(tree.root_node())?;
    Some(Insert { at: name.end_byte(), before: "\n  ", after: "" })
}
