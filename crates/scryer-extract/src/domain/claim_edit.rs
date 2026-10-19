//! Writing claims back into test files: rewording a test's description where
//! the lifter reads it (the `///` doc, the `it()` string, the docstring, the
//! DisplayName or XML summary), and writing a new claim as a todo stub in
//! the language's own form. Text in, text out; the caller does the I/O.

use crate::domain::lang;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Where a claim's test is: its identifier, the line it was lifted at, and
/// the concern tags to keep on the reworded description.
pub struct ClaimSite<'a> {
    pub identifier: &'a str,
    pub line: u32,
    pub concerns: &'a [String],
}

fn parse(rel_path: &str, source: &str) -> Result<tree_sitter::Tree, String> {
    let ext = lang::ext_of(Path::new(rel_path)).ok_or("file has no extension")?;
    let language = lang::language_for_ext(ext).ok_or_else(|| format!("no grammar for .{ext}"))?;
    let mut parser = Parser::new();
    parser.set_language(&language).map_err(|e| e.to_string())?;
    parser.parse(source, None).ok_or_else(|| "parse failed".to_string())
}

fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.byte_range()]
}

fn with_tags(text: &str, concerns: &[String]) -> String {
    let mut out = text.trim().to_string();
    for c in concerns {
        out.push_str(&format!(" [{c}]"));
    }
    out
}

fn splice(src: &str, start: usize, end: usize, with: &str) -> String {
    format!("{}{}{}", &src[..start], with, &src[end..])
}

/// The indentation of the line `byte` sits on.
fn indent_at(src: &str, byte: usize) -> &str {
    let line_start = src[..byte].rfind('\n').map_or(0, |i| i + 1);
    let line = &src[line_start..];
    &line[..line.len() - line.trim_start().len()]
}

fn line_start(src: &str, byte: usize) -> usize {
    src[..byte].rfind('\n').map_or(0, |i| i + 1)
}

/// Every node of `kind` in the tree, depth first.
fn all_of<'t>(root: Node<'t>, kinds: &[&str], out: &mut Vec<Node<'t>>) {
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if kinds.contains(&child.kind()) {
            out.push(child);
        }
        all_of(child, kinds, out);
    }
}

/// The candidate nearest the recorded line.
fn nearest<'t>(cands: Vec<Node<'t>>, line: u32) -> Option<Node<'t>> {
    cands
        .into_iter()
        .min_by_key(|n| (n.start_position().row as i64 + 1 - line as i64).abs())
}

/// Reword one test's description in `source`, keeping its concern tags.
pub fn rewrite_description(rel_path: &str, source: &str, site: &ClaimSite, new_text: &str) -> Result<String, String> {
    let new_text = with_tags(new_text, site.concerns);
    if new_text.contains('\n') {
        return Err("a claim is one line".into());
    }
    let tree = parse(rel_path, source)?;
    let root = tree.root_node();
    let ext = lang::ext_of(Path::new(rel_path)).unwrap_or_default();
    match ext {
        "rs" => rust_rewrite(root, source, site, &new_text),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs" => js_rewrite(root, source, site, &new_text),
        "py" => py_rewrite(root, source, site, &new_text),
        "cs" => cs_rewrite(root, source, site, &new_text),
        _ => Err(format!("cannot edit claims in .{ext} files")),
    }
}

fn rust_rewrite(root: Node, src: &str, site: &ClaimSite, new_text: &str) -> Result<String, String> {
    let mut fns = Vec::new();
    all_of(root, &["function_item"], &mut fns);
    let f = nearest(
        fns.into_iter()
            .filter(|f| f.child_by_field_name("name").is_some_and(|n| text(n, src) == site.identifier))
            .collect(),
        site.line,
    )
    .ok_or_else(|| format!("no fn {} in the file", site.identifier))?;
    // The doc lines and attributes directly above, top to bottom.
    let mut preamble = Vec::new();
    let mut prev = f.prev_sibling();
    while let Some(p) = prev {
        match p.kind() {
            "attribute_item" | "line_comment" | "block_comment" => preamble.push(p),
            _ => break,
        }
        prev = p.prev_sibling();
    }
    preamble.reverse();
    let docs: Vec<Node> = preamble
        .iter()
        .copied()
        .filter(|p| p.kind() == "line_comment" && text(*p, src).starts_with("///") && !text(*p, src).starts_with("////"))
        .collect();
    let indent = indent_at(src, f.start_byte()).to_string();
    match docs.first() {
        Some(first) => {
            // Replace the first paragraph, keep what follows a blank doc line.
            let para_end = docs
                .iter()
                .take_while(|d| !text(**d, src).trim_start_matches('/').trim().is_empty())
                .last()
                .unwrap_or(first);
            // A line comment's node carries its newline; keep it.
            let end = para_end.start_byte() + text(*para_end, src).trim_end().len();
            Ok(splice(src, first.start_byte(), end, &format!("/// {new_text}")))
        }
        None => {
            let top = preamble.first().copied().unwrap_or(f);
            let at = line_start(src, top.start_byte());
            Ok(splice(src, at, at, &format!("{indent}/// {new_text}\n")))
        }
    }
}

/// Quote `s` as a JS string literal in the quote style of `quote`.
fn js_quote(s: &str, quote: char) -> String {
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        if c == '\\' || c == quote {
            out.push('\\');
        }
        out.push(c);
    }
    if quote == '`' {
        out = out.replace("${", "\\${");
    }
    out.push(quote);
    out
}

fn js_string_content(n: Node, src: &str) -> Option<String> {
    matches!(n.kind(), "string" | "template_string").then(|| {
        let t = text(n, src);
        t[1..t.len().saturating_sub(1)].to_string()
    })
}

/// The base name of a runner call's callee: `it`, `test.skip` → `it`/`test`.
fn js_callee_base<'s>(callee: Node, src: &'s str) -> Option<&'s str> {
    match callee.kind() {
        "identifier" => Some(text(callee, src)),
        "member_expression" => js_callee_base(callee.child_by_field_name("object")?, src),
        "call_expression" => js_callee_base(callee.child_by_field_name("function")?, src),
        _ => None,
    }
}

fn js_rewrite(root: Node, src: &str, site: &ClaimSite, new_text: &str) -> Result<String, String> {
    let mut calls = Vec::new();
    all_of(root, &["call_expression"], &mut calls);
    let target = nearest(
        calls
            .into_iter()
            .filter_map(|c| {
                let base = js_callee_base(c.child_by_field_name("function")?, src)?;
                let base = base.trim_start_matches(['x', 'f']);
                if !matches!(base, "it" | "test" | "specify") {
                    return None;
                }
                let arg = c.child_by_field_name("arguments")?.named_child(0)?;
                (js_string_content(arg, src)? == site.identifier).then_some(arg)
            })
            .collect(),
        site.line,
    )
    .ok_or_else(|| format!("no test named {:?} in the file", site.identifier))?;
    let quote = text(target, src).chars().next().unwrap_or('"');
    Ok(splice(src, target.start_byte(), target.end_byte(), &js_quote(new_text, quote)))
}

fn py_rewrite(root: Node, src: &str, site: &ClaimSite, new_text: &str) -> Result<String, String> {
    let mut defs = Vec::new();
    all_of(root, &["function_definition"], &mut defs);
    let f = nearest(
        defs.into_iter()
            .filter(|f| f.child_by_field_name("name").is_some_and(|n| text(n, src) == site.identifier))
            .collect(),
        site.line,
    )
    .ok_or_else(|| format!("no def {} in the file", site.identifier))?;
    let body = f.child_by_field_name("body").ok_or("def has no body")?;
    let first = body.named_child(0).ok_or("def has an empty body")?;
    let escaped = new_text.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
    let docstring = (first.kind() == "expression_statement")
        .then(|| first.named_child(0))
        .flatten()
        .filter(|s| s.kind() == "string");
    match docstring {
        Some(s) => {
            // Keep any paragraphs after the first.
            let t = text(s, src);
            let open = ["\"\"\"", "'''", "\"", "'"].iter().find(|q| t.ends_with(**q)).copied().unwrap_or("\"\"\"");
            let inner_start = t.find(open).map_or(0, |i| i + open.len());
            let inner = &t[inner_start..t.len() - open.len()];
            let rest = inner.trim_start().find("\n\n").map(|i| &inner.trim_start()[i..]).unwrap_or("");
            let prefix = &t[..inner_start - open.len()];
            Ok(splice(src, s.start_byte(), s.end_byte(), &format!("{prefix}\"\"\"{escaped}{rest}\"\"\"")))
        }
        None => {
            let indent = indent_at(src, first.start_byte()).to_string();
            let at = first.start_byte();
            Ok(splice(src, at, at, &format!("\"\"\"{escaped}\"\"\"\n{indent}")))
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn cs_rewrite(root: Node, src: &str, site: &ClaimSite, new_text: &str) -> Result<String, String> {
    let mut methods = Vec::new();
    all_of(root, &["method_declaration"], &mut methods);
    let m = nearest(
        methods
            .into_iter()
            .filter(|m| m.child_by_field_name("name").is_some_and(|n| text(n, src) == site.identifier))
            .collect(),
        site.line,
    )
    .ok_or_else(|| format!("no method {} in the file", site.identifier))?;
    // A DisplayName is what the lifter reads first.
    let head = &src[m.start_byte()..m.child_by_field_name("name").map_or(m.end_byte(), |n| n.start_byte())];
    if let Some(at) = head.find("DisplayName") {
        let after = &head[at..];
        if let (Some(open), Some(_)) = (after.find('"'), after.find('=')) {
            let start = m.start_byte() + at + open + 1;
            let len = src[start..].find('"').ok_or("unterminated DisplayName")?;
            let escaped = new_text.replace('\\', "\\\\").replace('"', "\\\"");
            return Ok(splice(src, start, start + len, &escaped));
        }
    }
    // Else the `///` XML doc's summary, kept beside its other tags.
    let mut docs = Vec::new();
    let mut prev = m.prev_sibling();
    while let Some(p) = prev {
        if p.kind() != "comment" || !text(p, src).starts_with("///") {
            break;
        }
        docs.push(p);
        prev = p.prev_sibling();
    }
    docs.reverse();
    let indent = indent_at(src, m.start_byte()).to_string();
    let summary = format!("/// <summary>{}</summary>", xml_escape(new_text));
    let open = docs.iter().position(|d| text(*d, src).contains("<summary>"));
    let close = docs.iter().position(|d| text(*d, src).contains("</summary>"));
    match (open, close) {
        (Some(a), Some(b)) if b >= a => Ok(splice(src, docs[a].start_byte(), docs[b].end_byte(), &summary)),
        _ => {
            let top = docs.first().copied().unwrap_or(m);
            let at = line_start(src, top.start_byte());
            Ok(splice(src, at, at, &format!("{indent}{summary}\n")))
        }
    }
}

/// An identifier for a stub from its claim: the first words, snake_cased.
pub fn snake_name(claim: &str) -> String {
    let words: Vec<String> = claim
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| w.to_lowercase())
        .collect();
    let name = if words.is_empty() { "planned".to_string() } else { words.join("_") };
    if name.starts_with(|c: char| c.is_ascii_digit()) { format!("t_{name}") } else { name }
}

fn pascal_name(claim: &str) -> String {
    snake_name(claim)
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

/// `base` unless the file already defines it, else `base_2`, `base_3`…
fn unique(base: String, src: &str, taken: impl Fn(&str, &str) -> bool) -> String {
    if !taken(src, &base) {
        return base;
    }
    (2..).map(|i| format!("{base}_{i}")).find(|n| !taken(src, n)).unwrap()
}

/// Write `claim` as a todo stub into a test file (`source` is `None` for a
/// file that does not exist yet). `suite` names the describe a Vitest stub
/// goes inside when the file has one by that name.
pub fn insert_stub(rel_path: &str, source: Option<&str>, claim: &str, suite: Option<&str>) -> Result<String, String> {
    let claim = claim.trim();
    if claim.is_empty() || claim.contains('\n') {
        return Err("a claim is one non-empty line".into());
    }
    let ext = lang::ext_of(Path::new(rel_path)).unwrap_or_default();
    match ext {
        "rs" => rust_stub(rel_path, source, claim),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" | "mjs" | "cjs" => js_stub(rel_path, source, claim, suite),
        "py" => Ok(py_stub(source, claim)),
        "cs" => cs_stub(rel_path, source, claim),
        _ => Err(format!("cannot write stubs into .{ext} files")),
    }
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.is_empty() || s.ends_with('\n') { s.to_string() } else { format!("{s}\n") }
}

fn rust_stub(rel_path: &str, source: Option<&str>, claim: &str) -> Result<String, String> {
    let src = source.unwrap_or("");
    let name = unique(snake_name(claim), src, |s, n| s.contains(&format!("fn {n}(")));
    let stub = |indent: &str| {
        format!("{indent}/// {claim}\n{indent}#[test]\n{indent}#[ignore]\n{indent}fn {name}() {{\n{indent}    todo!()\n{indent}}}\n")
    };
    // An integration test file holds tests at top level.
    let integration = rel_path.split('/').any(|seg| seg == "tests");
    if integration || source.is_none() {
        let sep = if src.trim().is_empty() { "" } else { "\n" };
        return Ok(format!("{}{sep}{}", ensure_trailing_newline(src), stub("")));
    }
    let tree = parse(rel_path, src)?;
    let mut mods = Vec::new();
    all_of(tree.root_node(), &["mod_item"], &mut mods);
    let test_mod = mods.into_iter().find(|m| {
        let mut prev = m.prev_sibling();
        while let Some(p) = prev {
            if p.kind() != "attribute_item" {
                break;
            }
            if text(p, src).replace(' ', "") == "#[cfg(test)]" {
                return true;
            }
            prev = p.prev_sibling();
        }
        false
    });
    match test_mod.and_then(|m| m.child_by_field_name("body")) {
        Some(body) => {
            let close = body.end_byte() - 1;
            let inner_indent = format!("{}    ", indent_at(src, body.start_byte()));
            let before = src[..close].trim_end_matches([' ', '\t']);
            let lead = if before.ends_with('\n') { "\n" } else { "\n\n" };
            let outer = indent_at(src, close).to_string();
            Ok(format!("{before}{lead}{}{outer}{}", stub(&inner_indent), &src[close..]))
        }
        None => Ok(format!("{}\n#[cfg(test)]\nmod tests {{\n{}}}\n", ensure_trailing_newline(src), stub("    "))),
    }
}

fn js_stub(rel_path: &str, source: Option<&str>, claim: &str, suite: Option<&str>) -> Result<String, String> {
    let q = js_quote(claim, '"');
    let Some(src) = source else {
        let suite = js_quote(suite.unwrap_or("planned"), '"');
        return Ok(format!("import {{ describe, it }} from \"vitest\";\n\ndescribe({suite}, () => {{\n  it.todo({q});\n}});\n"));
    };
    let tree = parse(rel_path, src)?;
    let root = tree.root_node();
    let runner = if src.contains("it(") || src.contains("it.") { "it" } else { "test" };
    // Top-level describes, and the one the stub belongs in.
    let mut describes = Vec::new();
    let mut cursor = root.walk();
    for stmt in root.named_children(&mut cursor) {
        let Some(call) = (stmt.kind() == "expression_statement").then(|| stmt.named_child(0)).flatten() else { continue };
        if call.kind() != "call_expression" {
            continue;
        }
        let Some(base) = call.child_by_field_name("function").and_then(|f| js_callee_base(f, src)) else { continue };
        if base != "describe" {
            continue;
        }
        let args = call.child_by_field_name("arguments");
        let name = args.and_then(|a| a.named_child(0)).and_then(|n| js_string_content(n, src));
        let body = args
            .and_then(|a| a.named_child(1))
            .and_then(|f| f.child_by_field_name("body"))
            .filter(|b| b.kind() == "statement_block");
        if let Some(body) = body {
            describes.push((name, body));
        }
    }
    let target = suite
        .and_then(|s| describes.iter().find(|(n, _)| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(s))))
        .or(if describes.len() == 1 { describes.first() } else { None });
    match target {
        Some((_, body)) => {
            let close = body.end_byte() - 1;
            let indent = format!("{}  ", indent_at(src, body.start_byte()));
            let before = src[..close].trim_end_matches([' ', '\t']);
            let lead = if before.ends_with('\n') { "" } else { "\n" };
            let outer = indent_at(src, close).to_string();
            Ok(format!("{before}{lead}{indent}{runner}.todo({q});\n{outer}{}", &src[close..]))
        }
        None => Ok(format!("{}\n{runner}.todo({q});\n", ensure_trailing_newline(src))),
    }
}

fn py_stub(source: Option<&str>, claim: &str) -> String {
    let src = source.unwrap_or("");
    let name = unique(format!("test_{}", snake_name(claim)), src, |s, n| s.contains(&format!("def {n}(")));
    let doc = claim.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
    let stub = format!("@pytest.mark.skip(reason=\"planned\")\ndef {name}():\n    \"\"\"{doc}\"\"\"\n    ...\n");
    let has_import = src.lines().any(|l| l.trim() == "import pytest" || l.trim_start().starts_with("import pytest"));
    let head = if has_import {
        ensure_trailing_newline(src)
    } else if src.trim().is_empty() {
        "import pytest\n".to_string()
    } else {
        format!("import pytest\n{}", ensure_trailing_newline(src))
    };
    let sep = if head.trim().is_empty() || head.trim() == "import pytest" { "\n\n" } else { "\n\n" };
    format!("{}{sep}{stub}", head.trim_end_matches('\n'))
}

fn cs_stub(rel_path: &str, source: Option<&str>, claim: &str) -> Result<String, String> {
    let src = source.unwrap_or("");
    let name = unique(pascal_name(claim), src, |s, n| s.contains(&format!(" {n}(")));
    let summary = xml_escape(claim);
    let stub = |indent: &str| {
        format!("{indent}/// <summary>{summary}</summary>\n{indent}[Fact(Skip = \"planned\")]\n{indent}public void {name}()\n{indent}{{\n{indent}}}\n")
    };
    let Some(src) = source else {
        let path = Path::new(rel_path);
        let class = path.file_stem().and_then(|s| s.to_str()).unwrap_or("PlannedTests");
        let ns = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("Tests");
        return Ok(format!("using Xunit;\n\nnamespace {ns};\n\npublic class {class}\n{{\n{}}}\n", stub("    ")));
    };
    let tree = parse(rel_path, src)?;
    let mut classes = Vec::new();
    all_of(tree.root_node(), &["class_declaration"], &mut classes);
    let body = classes
        .last()
        .and_then(|c| c.child_by_field_name("body"))
        .ok_or("no class to add the test to")?;
    let close = body.end_byte() - 1;
    let indent = format!("{}    ", indent_at(src, body.start_byte()));
    let before = src[..close].trim_end_matches([' ', '\t']);
    let lead = if before.trim_end().ends_with('{') { "" } else { "\n" };
    let before = if before.ends_with('\n') { before.to_string() } else { format!("{before}\n") };
    let outer = indent_at(src, close).to_string();
    Ok(format!("{before}{lead}{}{outer}{}", stub(&indent), &src[close..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::claims::lift_file;

    fn lifted(path: &str, src: &str) -> Vec<(String, bool)> {
        lift_file(path, src, &mut Parser::new())
            .tests
            .into_iter()
            .map(|t| (format!("{}{}", t.description, t.concerns.iter().map(|c| format!(" [{c}]")).collect::<String>()), t.planned))
            .collect()
    }

    fn site<'a>(identifier: &'a str, line: u32, concerns: &'a [String]) -> ClaimSite<'a> {
        ClaimSite { identifier, line, concerns }
    }

    /// When a Rust test's claim is reworded, its first doc paragraph is
    /// replaced and its tags and later paragraphs kept; a test with no doc
    /// gets one above its attributes.
    #[test]
    fn rewords_rust_doc_comments() {
        let src = "mod tests {\n    /// Old claim\n    ///\n    /// Detail.\n    #[test]\n    fn a() {}\n\n    #[test]\n    fn b() {}\n}\n";
        let tags = vec!["io".to_string()];
        let out = rewrite_description("src/x.rs", src, &site("a", 6, &tags), "When saving, keeps it").unwrap();
        assert!(out.contains("    /// When saving, keeps it [io]\n    ///\n    /// Detail.\n"), "{out}");
        let out = rewrite_description("src/x.rs", &out, &site("b", 9, &[]), "Loads it").unwrap();
        assert!(out.contains("    /// Loads it\n    #[test]\n    fn b()"), "{out}");
        assert_eq!(lifted("src/x.rs", &out), vec![("When saving, keeps it [io]".into(), false), ("Loads it".into(), false)]);
    }

    /// When a Vitest claim is reworded, the it() string changes in its own
    /// quote style, escaped.
    #[test]
    fn rewords_js_it_strings() {
        let src = "describe('tree', () => {\n  it('hides children', () => {});\n});\n";
        let out = rewrite_description("a.test.ts", src, &site("hides children", 2, &[]), "hides the user's children").unwrap();
        assert!(out.contains(r"it('hides the user\'s children', () => {});"), "{out}");
        assert_eq!(lifted("a.test.ts", &out), vec![("hides the user's children".into(), false)]);
    }

    /// When a pytest claim is reworded, its docstring changes, or one is
    /// added as the first statement.
    #[test]
    fn rewords_pytest_docstrings() {
        let src = "def test_a():\n    \"\"\"Old\"\"\"\n    assert 1\n\ndef test_b():\n    assert 1\n";
        let out = rewrite_description("test_x.py", src, &site("test_a", 1, &[]), "Rounds half up").unwrap();
        let out = rewrite_description("test_x.py", &out, &site("test_b", 5, &[]), "Refunds in full").unwrap();
        assert_eq!(
            lifted("test_x.py", &out),
            vec![("Rounds half up".into(), false), ("Refunds in full".into(), false)]
        );
    }

    /// When a .NET claim is reworded, a DisplayName changes if present, else
    /// the XML summary, else a summary is added above the attributes.
    #[test]
    fn rewords_dotnet_display_names_and_summaries() {
        let src = "public class T\n{\n    [Fact(DisplayName = \"Old\")]\n    public void A() { }\n\n    /// <summary>Old</summary>\n    /// <remarks>keep</remarks>\n    [Fact]\n    public void B() { }\n\n    [Fact]\n    public void C() { }\n}\n";
        let out = rewrite_description("XTests.cs", src, &site("A", 4, &[]), "Docks in range").unwrap();
        let out = rewrite_description("XTests.cs", &out, &site("B", 9, &[]), "Refuses <far> rigs").unwrap();
        let out = rewrite_description("XTests.cs", &out, &site("C", 12, &[]), "Refuels").unwrap();
        assert!(out.contains("<remarks>keep</remarks>"), "{out}");
        assert_eq!(
            lifted("XTests.cs", &out),
            vec![("Docks in range".into(), false), ("Refuses &lt;far&gt; rigs".into(), false), ("Refuels".into(), false)]
        );
    }

    /// When a claim is added to a Rust file, it lands as an ignored todo test
    /// in the file's test module, creating the module when missing.
    #[test]
    fn writes_rust_stubs_into_the_test_module() {
        let src = "pub fn save() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn saves() {}\n}\n";
        let out = insert_stub("src/draft.rs", Some(src), "When offline, queues the save", None).unwrap();
        assert!(out.contains("    /// When offline, queues the save\n    #[test]\n    #[ignore]\n    fn when_offline_queues_the_save() {\n        todo!()\n    }\n}\n"), "{out}");
        let fresh = insert_stub("src/store.rs", Some("pub fn put() {}\n"), "Puts", None).unwrap();
        assert!(fresh.ends_with("#[cfg(test)]\nmod tests {\n    /// Puts\n    #[test]\n    #[ignore]\n    fn puts() {\n        todo!()\n    }\n}\n"), "{fresh}");
        let again = insert_stub("src/store.rs", Some(&fresh), "Puts", None).unwrap();
        assert!(again.contains("fn puts_2()"));
        assert_eq!(lifted("src/draft.rs", &out).last().unwrap(), &("When offline, queues the save".to_string(), true));
    }

    /// When a claim is added to a Vitest file, it.todo lands inside the
    /// matching describe, or a new file is written around it.
    #[test]
    fn writes_vitest_todos_inside_the_describe() {
        let src = "import { describe, it } from \"vitest\";\n\ndescribe(\"tree\", () => {\n  it(\"hides\", () => {});\n});\n";
        let out = insert_stub("tree.test.ts", Some(src), "scrolls to \"selection\"", Some("tree")).unwrap();
        assert!(out.contains("  it(\"hides\", () => {});\n  it.todo(\"scrolls to \\\"selection\\\"\");\n});\n"), "{out}");
        let fresh = insert_stub("new.test.ts", None, "renders", Some("new")).unwrap();
        assert_eq!(lifted("new.test.ts", &fresh), vec![("renders".into(), true)]);
        assert_eq!(lifted("tree.test.ts", &out)[1], ("scrolls to \"selection\"".into(), true));
    }

    /// When a claim is added to a pytest file, a skipped test with the claim
    /// as its docstring is appended and pytest imported.
    #[test]
    fn writes_pytest_skips() {
        let out = insert_stub("tests/test_price.py", Some("def test_a():\n    assert 1\n"), "Rounds half up", None).unwrap();
        assert!(out.starts_with("import pytest\n"));
        assert!(out.ends_with("@pytest.mark.skip(reason=\"planned\")\ndef test_rounds_half_up():\n    \"\"\"Rounds half up\"\"\"\n    ...\n"), "{out}");
        assert_eq!(lifted("tests/test_price.py", &out)[1], ("Rounds half up".into(), true));
    }

    /// When a claim is added to a .NET test class, a skipped Fact with the
    /// claim as its summary goes at the end of the class; a new file gets a
    /// class of its own.
    #[test]
    fn writes_dotnet_skipped_facts() {
        let src = "namespace S.Tests;\n\npublic class DockTests\n{\n    [Fact]\n    public void Docks() { }\n}\n";
        let out = insert_stub("tests/S.Tests/DockTests.cs", Some(src), "Refuses far rigs", None).unwrap();
        assert!(out.contains("    /// <summary>Refuses far rigs</summary>\n    [Fact(Skip = \"planned\")]\n    public void RefusesFarRigs()\n    {\n    }\n}\n"), "{out}");
        assert_eq!(lifted("tests/S.Tests/DockTests.cs", &out)[1], ("Refuses far rigs".into(), true));
        let fresh = insert_stub("tests/S.Tests/FuelTests.cs", None, "Refuels", None).unwrap();
        assert!(fresh.contains("namespace S.Tests;") && fresh.contains("public class FuelTests"));
        assert_eq!(lifted("tests/S.Tests/FuelTests.cs", &fresh), vec![("Refuels".into(), true)]);
    }
}
