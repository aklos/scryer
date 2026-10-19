//! Behaviour tests read as claims, and modules read as descriptions.
//!
//! A test's description is its claim: the `///` doc on a Rust `#[test]` fn,
//! the `it("…")` string in Vitest/Jest, a pytest function's docstring, a .NET
//! test's `DisplayName` or XML doc. With none of those, the identifier
//! de-snaked stands in. A todo or skipped test is a planned claim. The module
//! doc (`//!`, a file-head JSDoc, a module docstring) is the description of
//! the node the file belongs to and carries no status.

use crate::domain::lang;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// One behaviour test as a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiftedTest {
    pub line: u32,
    pub end_line: u32,
    /// The code identifier: fn/method name, or the `it` string in JS.
    pub identifier: String,
    /// The claim text, trailing `[tag]`s stripped.
    pub description: String,
    pub concerns: Vec<String>,
    /// The leaf name a JUnit report gives this test.
    pub junit_name: String,
    /// A todo/skip stub, or a body that only says "not yet".
    pub planned: bool,
    /// The description was written, not de-snaked from the identifier.
    pub documented: bool,
}

/// Everything one file says about claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLift {
    pub tests: Vec<LiftedTest>,
    pub module_description: Option<String>,
    /// Line spans that are test code inside a product file (Rust
    /// `#[cfg(test)] mod`), so their helpers never become symbols.
    pub test_spans: Vec<(u32, u32)>,
}

/// Lift the claims and module description from one source file.
pub fn lift_file(rel_path: &str, source: &str, parser: &mut Parser) -> FileLift {
    let Some(ext) = lang::ext_of(Path::new(rel_path)) else {
        return FileLift::default();
    };
    let Some(language) = lang::language_for_ext(ext) else {
        return FileLift::default();
    };
    if parser.set_language(&language).is_err() {
        return FileLift::default();
    }
    let Some(tree) = parser.parse(source, None) else {
        return FileLift::default();
    };
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let mut lift = FileLift::default();
    match ext {
        "rs" => {
            rust_tests(root, bytes, &mut lift);
            lift.module_description = rust_module_doc(root, bytes);
        }
        "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            js_tests(root, bytes, &mut Vec::new(), false, &mut lift.tests);
            lift.module_description = js_module_doc(root, bytes);
        }
        "py" => {
            py_tests(root, bytes, false, &mut lift.tests);
            lift.module_description = py_module_doc(root, bytes);
        }
        "cs" => cs_tests(root, bytes, &mut lift.tests),
        _ => {}
    }
    // A `[manual]` test is a tool run by hand, not a claim.
    lift.tests.retain(|t| !t.concerns.iter().any(|c| c == MANUAL_TAG));
    lift.tests.sort_by_key(|t| t.line);
    lift
}

/// The tag that keeps a test out of the claims: an ad-hoc tool, run by hand.
pub const MANUAL_TAG: &str = "manual";

/// Split trailing `[tag]`s off a description: `keeps the draft [persistence]`
/// → (`keeps the draft`, [`persistence`]).
pub fn split_concerns(text: &str) -> (String, Vec<String>) {
    let mut rest = text.trim_end();
    let mut tags = Vec::new();
    while let Some(head) = rest.strip_suffix(']') {
        let Some(open) = head.rfind('[') else { break };
        let tag = head[open + 1..].trim();
        if tag.is_empty() || tag.contains(']') {
            break;
        }
        tags.push(tag.to_string());
        rest = head[..open].trim_end();
    }
    tags.reverse();
    (rest.to_string(), tags)
}

/// An identifier read as words: `keeps_draft_when_save_fails`,
/// `KeepsDraftWhenSaveFails` and `test_keeps_draft` all become
/// `keeps draft …`. Acronyms keep their case.
pub fn de_snake(identifier: &str) -> String {
    let trimmed = identifier
        .strip_prefix("test_")
        .or_else(|| identifier.strip_prefix("Test").filter(|r| r.starts_with(|c: char| c.is_uppercase() || c == '_')))
        .unwrap_or(identifier);
    let mut words: Vec<String> = Vec::new();
    for part in trimmed.split(|c: char| c == '_' || c == '-' || c.is_whitespace()) {
        let chars: Vec<char> = part.chars().collect();
        let mut word = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let boundary = i > 0
                && c.is_uppercase()
                && (chars[i - 1].is_lowercase()
                    || chars[i - 1].is_ascii_digit()
                    || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
            if boundary && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(c);
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    words
        .into_iter()
        .map(|w| {
            let acronym = w.len() > 1 && w.chars().all(|c| c.is_uppercase() || c.is_ascii_digit());
            if acronym { w } else { w.to_lowercase() }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn text<'a>(node: Node, bytes: &'a [u8]) -> &'a str {
    node.utf8_text(bytes).unwrap_or_default()
}

fn line_of(node: Node) -> (u32, u32) {
    (node.start_position().row as u32 + 1, node.end_position().row as u32 + 1)
}

/// The first paragraph of a doc, its lines joined and whitespace collapsed.
fn first_paragraph(lines: &[String]) -> Option<String> {
    let para: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .skip_while(|l| l.is_empty())
        .take_while(|l| !l.is_empty())
        .collect();
    let joined = para.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
    (!joined.is_empty()).then_some(joined)
}

fn make_test(
    node: Node,
    identifier: String,
    doc: Option<String>,
    junit_name: String,
    planned: bool,
) -> LiftedTest {
    let (line, end_line) = line_of(node);
    let documented = doc.is_some();
    let raw = doc.unwrap_or_else(|| de_snake(&identifier));
    let (description, concerns) = split_concerns(&raw);
    LiftedTest { line, end_line, identifier, description, concerns, junit_name, planned, documented }
}

// --- Rust ---

/// The path an attribute names: `#[tokio::test]` → `tokio::test`.
fn rust_attr_path(attr: &str) -> &str {
    let inner = attr.trim().trim_start_matches("#[").trim_end_matches(']').trim();
    inner
        .split(|c: char| c == '(' || c == '=' || c.is_whitespace())
        .next()
        .unwrap_or_default()
}

fn rust_tests(node: Node, bytes: &[u8], lift: &mut FileLift) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                let (attrs, docs) = rust_preamble(child, bytes);
                let is_test = attrs.iter().any(|a| {
                    let leaf = rust_attr_path(a).rsplit("::").next().unwrap_or_default();
                    matches!(leaf, "test" | "rstest" | "test_case" | "quickcheck")
                });
                if !is_test {
                    continue;
                }
                let name = child
                    .child_by_field_name("name")
                    .map(|n| text(n, bytes).to_string())
                    .unwrap_or_default();
                let ignored = attrs.iter().any(|a| rust_attr_path(a) == "ignore");
                let planned = ignored || rust_body_is_todo(child, bytes);
                let doc = first_paragraph(&docs);
                lift.tests.push(make_test(child, name.clone(), doc, name, planned));
            }
            "mod_item" => {
                let (attrs, _) = rust_preamble(child, bytes);
                if attrs.iter().any(|a| a.replace(' ', "") == "#[cfg(test)]") {
                    lift.test_spans.push(line_of(child));
                }
                if let Some(body) = child.child_by_field_name("body") {
                    rust_tests(body, bytes, lift);
                }
            }
            _ => {}
        }
    }
}

/// The attributes and `///` doc lines directly above an item, top to bottom.
fn rust_preamble(item: Node, bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut attrs = Vec::new();
    let mut docs = Vec::new();
    let mut prev = item.prev_sibling();
    while let Some(p) = prev {
        let t = text(p, bytes);
        match p.kind() {
            "attribute_item" => attrs.push(t.to_string()),
            "line_comment" if t.starts_with("///") && !t.starts_with("////") => {
                docs.push(t.trim_start_matches("///").trim_end().to_string())
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        prev = p.prev_sibling();
    }
    attrs.reverse();
    docs.reverse();
    (attrs, docs)
}

/// A body whose only statement is `todo!()` or `unimplemented!()`.
fn rust_body_is_todo(func: Node, bytes: &[u8]) -> bool {
    let Some(body) = func.child_by_field_name("body") else { return false };
    let mut cursor = body.walk();
    let stmts: Vec<Node> = body
        .named_children(&mut cursor)
        .filter(|n| !n.kind().ends_with("comment"))
        .collect();
    let [only] = stmts.as_slice() else { return false };
    let mac = if only.kind() == "expression_statement" { only.named_child(0) } else { Some(*only) };
    mac.filter(|m| m.kind() == "macro_invocation")
        .and_then(|m| m.child_by_field_name("macro"))
        .is_some_and(|name| matches!(text(name, bytes), "todo" | "unimplemented"))
}

/// The leading `//!` lines (or a `/*! */` block) of a Rust file.
fn rust_module_doc(root: Node, bytes: &[u8]) -> Option<String> {
    let mut lines = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let t = text(child, bytes);
        match child.kind() {
            "line_comment" if t.starts_with("//!") => {
                lines.push(t.trim_start_matches("//!").trim_end().to_string())
            }
            "block_comment" if t.starts_with("/*!") => {
                lines.extend(
                    t.trim_start_matches("/*!")
                        .trim_end_matches("*/")
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').to_string()),
                );
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
    }
    first_paragraph(&lines)
}

// --- Vitest / Jest ---

/// What a test-runner call is: `it`, `test.skip`, `it.each(...)`, `describe.todo`.
struct JsCall<'a> {
    base: &'a str,
    modifiers: Vec<&'a str>,
}

fn js_call_shape<'a>(callee: Node, bytes: &'a [u8]) -> Option<JsCall<'a>> {
    match callee.kind() {
        "identifier" => Some(JsCall { base: text(callee, bytes), modifiers: Vec::new() }),
        "member_expression" => {
            let object = callee.child_by_field_name("object")?;
            let property = callee.child_by_field_name("property")?;
            let mut shape = js_call_shape(object, bytes)?;
            shape.modifiers.push(text(property, bytes));
            Some(shape)
        }
        // `it.each(table)("name", fn)`: the callee is itself a call.
        "call_expression" => js_call_shape(callee.child_by_field_name("function")?, bytes),
        _ => None,
    }
}

fn js_string(node: Node, bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "string" | "template_string" => {
            let t = text(node, bytes);
            Some(t[1..t.len().saturating_sub(1)].to_string())
        }
        _ => None,
    }
}

fn js_tests(
    node: Node,
    bytes: &[u8],
    suite: &mut Vec<String>,
    skipped: bool,
    out: &mut Vec<LiftedTest>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "call_expression" {
            let shape = child
                .child_by_field_name("function")
                .and_then(|f| js_call_shape(f, bytes));
            let args = child.child_by_field_name("arguments");
            let name = args
                .and_then(|a| a.named_child(0))
                .and_then(|n| js_string(n, bytes));
            if let (Some(shape), Some(name)) = (shape, name) {
                let skip_here = shape.modifiers.iter().any(|m| matches!(*m, "skip" | "todo"))
                    || shape.base.starts_with('x');
                let base = shape.base.trim_start_matches(['x', 'f']);
                match base {
                    "describe" | "suite" | "context" => {
                        suite.push(name);
                        if let Some(args) = args {
                            js_tests(args, bytes, suite, skipped || skip_here, out);
                        }
                        suite.pop();
                        continue;
                    }
                    "it" | "test" | "specify" => {
                        let junit = suite
                            .iter()
                            .cloned()
                            .chain(std::iter::once(name.clone()))
                            .collect::<Vec<_>>()
                            .join(" > ");
                        let planned = skipped || skip_here;
                        out.push(make_test(child, name.clone(), Some(name), junit, planned));
                        continue;
                    }
                    _ => {}
                }
            }
        }
        js_tests(child, bytes, suite, skipped, out);
    }
}

/// Clean a `/** … */` block into its prose lines, tags dropped.
fn jsdoc_lines(comment: &str) -> Vec<String> {
    comment
        .trim_start_matches("/**")
        .trim_end_matches("*/")
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim().to_string())
        .map(|l| {
            // `@file`/`@module`/`@fileoverview` introduce the description itself.
            for tag in ["@fileoverview", "@file", "@module", "@overview"] {
                if let Some(rest) = l.strip_prefix(tag) {
                    return rest.trim().to_string();
                }
            }
            l
        })
        .take_while(|l| !l.starts_with('@'))
        .collect()
}

/// A JSDoc at the head of the file that documents the file, not the first
/// declaration under it.
fn js_module_doc(root: Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = root.walk();
    let children: Vec<Node> = root.named_children(&mut cursor).collect();
    for (i, child) in children.iter().enumerate() {
        if child.kind() == "hash_bang_line" {
            continue;
        }
        if child.kind() != "comment" {
            return None;
        }
        let t = text(*child, bytes);
        if !t.starts_with("/**") {
            continue;
        }
        let tagged = ["@file", "@module", "@overview"].iter().any(|tag| t.contains(tag));
        let attached = children.get(i + 1).is_some_and(|next| {
            next.kind() != "import_statement"
                && next.kind() != "comment"
                && next.start_position().row == child.end_position().row + 1
        });
        if attached && !tagged {
            return None;
        }
        return first_paragraph(&jsdoc_lines(t));
    }
    None
}

// --- pytest ---

fn py_string(node: Node, bytes: &[u8]) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let mut cursor = node.walk();
    let content: String = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "string_content")
        .map(|c| text(c, bytes))
        .collect();
    Some(content)
}

/// The docstring opening a block, when there is one.
fn py_docstring(block: Node, bytes: &[u8]) -> Option<String> {
    let first = block.named_child(0)?;
    if first.kind() != "expression_statement" {
        return None;
    }
    py_string(first.named_child(0)?, bytes)
}

fn py_tests(node: Node, bytes: &[u8], in_test_class: bool, out: &mut Vec<LiftedTest>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let (def, decorators) = match child.kind() {
            "decorated_definition" => {
                let mut c = child.walk();
                let decorators: Vec<String> = child
                    .named_children(&mut c)
                    .filter(|d| d.kind() == "decorator")
                    .map(|d| text(d, bytes).trim_start_matches('@').trim().to_string())
                    .collect();
                (child.child_by_field_name("definition"), decorators)
            }
            _ => (Some(child), Vec::new()),
        };
        let Some(def) = def else { continue };
        let name = def
            .child_by_field_name("name")
            .map(|n| text(n, bytes).to_string())
            .unwrap_or_default();
        match def.kind() {
            "function_definition" if name.starts_with("test") && (in_test_class || node.kind() == "module") => {
                let skipped = decorators.iter().any(|d| {
                    let path = d.split('(').next().unwrap_or_default().trim();
                    path.ends_with("mark.skip") || path == "unittest.skip" || path == "skip"
                });
                let body = def.child_by_field_name("body");
                let doc = body
                    .and_then(|b| py_docstring(b, bytes))
                    .map(|d| d.lines().map(str::to_string).collect::<Vec<_>>())
                    .and_then(|lines| first_paragraph(&lines));
                let stub = body.is_some_and(|b| py_body_is_stub(b, bytes));
                out.push(make_test(child, name.clone(), doc, name, skipped || stub));
            }
            "class_definition" if name.starts_with("Test") => {
                if let Some(body) = def.child_by_field_name("body") {
                    py_tests(body, bytes, true, out);
                }
            }
            _ => {}
        }
    }
}

/// A body that, past its docstring, only passes or raises NotImplementedError.
fn py_body_is_stub(block: Node, bytes: &[u8]) -> bool {
    let mut cursor = block.walk();
    let stmts: Vec<Node> = block
        .named_children(&mut cursor)
        .filter(|n| n.kind() != "comment")
        .collect();
    let rest = match stmts.first() {
        Some(first)
            if first.kind() == "expression_statement"
                && first.named_child(0).is_some_and(|s| s.kind() == "string") =>
        {
            &stmts[1..]
        }
        _ => &stmts[..],
    };
    match rest {
        [] => true,
        [only] => {
            let t = text(*only, bytes).trim();
            t == "pass" || t == "..." || (only.kind() == "raise_statement" && t.contains("NotImplementedError"))
        }
        _ => false,
    }
}

fn py_module_doc(root: Node, bytes: &[u8]) -> Option<String> {
    let doc = py_docstring(root, bytes)?;
    first_paragraph(&doc.lines().map(str::to_string).collect::<Vec<_>>())
}

// --- .NET ---

const CS_TEST_ATTRS: &[&str] = &["Fact", "Theory", "Test", "TestCase", "TestMethod", "DataTestMethod", "SkippableFact"];

/// An attribute's bare name: `Xunit.FactAttribute` → `Fact`.
fn cs_attr_name(attr: Node, bytes: &[u8]) -> String {
    let name = attr.child_by_field_name("name").map(|n| text(n, bytes)).unwrap_or_default();
    let leaf = name.rsplit('.').next().unwrap_or(name);
    leaf.strip_suffix("Attribute").unwrap_or(leaf).to_string()
}

/// The string a named attribute argument sets: `DisplayName = "…"`.
fn cs_named_arg(attr: Node, bytes: &[u8], key: &str) -> Option<String> {
    let t = text(attr, bytes);
    let at = t.find(key)?;
    let rest = t[at + key.len()..].trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('@').unwrap_or(rest).strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

fn cs_tests(node: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "method_declaration" {
            cs_tests(child, bytes, out);
            continue;
        }
        let mut attrs = Vec::new();
        let mut c = child.walk();
        for list in child.named_children(&mut c).filter(|n| n.kind() == "attribute_list") {
            let mut lc = list.walk();
            attrs.extend(list.named_children(&mut lc).filter(|a| a.kind() == "attribute"));
        }
        let test_attr = attrs.iter().find(|a| CS_TEST_ATTRS.contains(&cs_attr_name(**a, bytes).as_str()));
        let Some(test_attr) = test_attr else { continue };
        let name = child
            .child_by_field_name("name")
            .map(|n| text(n, bytes).to_string())
            .unwrap_or_default();
        let skipped = cs_named_arg(*test_attr, bytes, "Skip").is_some()
            || attrs.iter().any(|a| matches!(cs_attr_name(*a, bytes).as_str(), "Skip" | "Ignore"));
        let stub = child.child_by_field_name("body").is_some_and(|b| {
            let mut bc = b.walk();
            let stmts: Vec<Node> = b.named_children(&mut bc).filter(|n| n.kind() != "comment").collect();
            matches!(stmts.as_slice(), [only] if text(*only, bytes).contains("NotImplementedException"))
        });
        let doc = cs_named_arg(*test_attr, bytes, "DisplayName").or_else(|| cs_xml_summary(child, bytes));
        out.push(make_test(child, name.clone(), doc, name, skipped || stub));
    }
}

/// The `<summary>` of the `///` XML doc directly above a member, tags
/// reduced to their text (`<paramref name="km"/>` → `km`).
fn cs_xml_summary(member: Node, bytes: &[u8]) -> Option<String> {
    let mut lines = Vec::new();
    let mut prev = member.prev_sibling();
    while let Some(p) = prev {
        let t = text(p, bytes);
        if p.kind() != "comment" || !t.starts_with("///") {
            break;
        }
        lines.push(t.trim_start_matches("///").trim().to_string());
        prev = p.prev_sibling();
    }
    lines.reverse();
    let xml = lines.join(" ");
    let body = match (xml.find("<summary>"), xml.find("</summary>")) {
        (Some(a), Some(b)) if b > a => &xml[a + "<summary>".len()..b],
        _ => xml.as_str(),
    };
    let plain = strip_xml_tags(body);
    first_paragraph(&[plain])
}

fn strip_xml_tags(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open..];
            break;
        };
        let tag = &rest[open + 1..open + close];
        if tag.ends_with('/') {
            // A self-closing reference stands for the name it points at.
            for key in ["name=\"", "cref=\"", "langword=\""] {
                if let Some(at) = tag.find(key) {
                    let v = &tag[at + key.len()..];
                    let v = &v[..v.find('"').unwrap_or(v.len())];
                    out.push_str(v.rsplit('.').next().unwrap_or(v));
                    break;
                }
            }
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lift(path: &str, src: &str) -> FileLift {
        lift_file(path, src, &mut Parser::new())
    }

    /// When a Rust test fn carries a `///` doc, the doc's first paragraph is
    /// the claim and trailing tags become concerns.
    #[test]
    fn rust_doc_comment_is_the_claim() {
        let src = r#"
//! Holds the user's draft.
//!
//! More detail nobody reads.

pub fn save() {}

#[cfg(test)]
mod tests {
    /// When a save fails, keeps the draft [persistence]
    #[test]
    fn keeps_draft() {}

    #[tokio::test]
    async fn retries_after_timeout() {}

    fn helper() {}
}
"#;
        let l = lift("src/draft.rs", src);
        assert_eq!(l.module_description.as_deref(), Some("Holds the user's draft."));
        assert_eq!(l.tests.len(), 2, "helper is not a test: {:?}", l.tests);
        let t = &l.tests[0];
        assert_eq!(t.identifier, "keeps_draft");
        assert_eq!(t.description, "When a save fails, keeps the draft");
        assert_eq!(t.concerns, vec!["persistence"]);
        assert_eq!(t.junit_name, "keeps_draft");
        assert!(t.documented && !t.planned);
        assert_eq!(l.tests[1].description, "retries after timeout");
        assert!(!l.tests[1].documented);
        assert_eq!(l.test_spans.len(), 1, "the cfg(test) module is test code");
    }

    /// If a Rust test is `#[ignore]`d or its body is only `todo!()` or
    /// `unimplemented!()`, it is planned.
    #[test]
    fn rust_stubs_are_planned() {
        let src = r#"
#[test]
#[ignore]
fn a() { assert!(true); }
#[test]
fn b() { todo!() }
#[test]
fn c() { unimplemented!("later"); }
#[test]
fn d() { todo!(); assert!(false); }
"#;
        let l = lift("tests/x.rs", src);
        let planned: Vec<bool> = l.tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![true, true, true, false]);
    }

    /// When a Vitest/Jest test sits inside describes, the `it` string is the
    /// claim and the JUnit name joins the suite path with ` > `; todo and skip
    /// variants, and everything inside a skipped describe, are planned.
    #[test]
    fn js_it_string_is_the_claim() {
        let src = r#"
/**
 * @file Renders the tree.
 */
import { describe, it, test } from "vitest";

describe("tree", () => {
  describe("when collapsed", () => {
    it("hides children [ui]", () => { expect(1).toBe(1); });
    it.todo("remembers the fold");
  });
  test.each([1, 2])("renders %s rows", (n) => {});
  it.skip("scrolls to selection", () => {});
});
describe.skip("later", () => { it("does a thing", () => {}); });
"#;
        let l = lift("src/tree.test.ts", src);
        assert_eq!(l.module_description.as_deref(), Some("Renders the tree."));
        let got: Vec<(&str, &str, bool)> = l
            .tests
            .iter()
            .map(|t| (t.description.as_str(), t.junit_name.as_str(), t.planned))
            .collect();
        assert_eq!(
            got,
            vec![
                ("hides children", "tree > when collapsed > hides children [ui]", false),
                ("remembers the fold", "tree > when collapsed > remembers the fold", true),
                ("renders %s rows", "tree > renders %s rows", false),
                ("scrolls to selection", "tree > scrolls to selection", true),
                ("does a thing", "later > does a thing", true),
            ]
        );
        assert_eq!(l.tests[0].concerns, vec!["ui"]);
    }

    /// A JSDoc directly above the first declaration documents the declaration,
    /// not the file.
    #[test]
    fn js_declaration_doc_is_not_a_module_doc() {
        let src = "/** Adds. */\nexport function add() {}\n";
        assert_eq!(lift("src/add.ts", src).module_description, None);
        let src = "/** Math helpers. */\n\nexport function add() {}\n";
        assert_eq!(lift("src/add.ts", src).module_description.as_deref(), Some("Math helpers."));
    }

    /// When a pytest function has a docstring it is the claim; without one the
    /// name is de-snaked. Skip marks and pass-only bodies are planned, and a
    /// parametrised test is one claim.
    #[test]
    fn pytest_docstring_is_the_claim() {
        let src = r#"
"""Prices a quote."""
import pytest

def test_rounds_to_cents():
    """When a total has fractions of a cent, rounds half up [money]"""
    assert price(1.005) == 1.01

@pytest.mark.parametrize("n", [1, 2, 3])
def test_scales_linearly(n):
    assert price(n) == n

@pytest.mark.skip(reason="later")
def test_applies_discount():
    assert False

class TestRefunds:
    def test_refunds_in_full(self):
        """Refunds the whole amount"""
        pass

def helper():
    pass
"#;
        let l = lift("tests/test_price.py", src);
        assert_eq!(l.module_description.as_deref(), Some("Prices a quote."));
        let got: Vec<(&str, bool)> = l.tests.iter().map(|t| (t.description.as_str(), t.planned)).collect();
        assert_eq!(
            got,
            vec![
                ("When a total has fractions of a cent, rounds half up", false),
                ("scales linearly", false),
                ("applies discount", true),
                ("Refunds the whole amount", true),
            ]
        );
        assert_eq!(l.tests[0].concerns, vec!["money"]);
        assert_eq!(l.tests[1].junit_name, "test_scales_linearly");
    }

    /// When a .NET test has a DisplayName it is the claim, else its XML doc
    /// summary, else its de-snaked name; Skip and NotImplementedException
    /// bodies are planned.
    #[test]
    fn dotnet_display_name_or_xml_doc_is_the_claim() {
        let src = r#"
namespace Game.Tests;

public class DockTests
{
    /// <summary>When a rig is <paramref name="km"/> out, refuses docking</summary>
    [Fact]
    public void RefusesFarRigs() { Assert.True(true); }

    [Fact(DisplayName = "Docks a rig in range [docking]")]
    public void DocksNearRigs() { }

    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    public void Charges_fee_per_hour(int h) { }

    [Fact(Skip = "not yet")]
    public void Undocks() { }

    [Fact]
    public void Refuels() { throw new NotImplementedException(); }

    public void Helper() { }
}
"#;
        let l = lift("tests/Game.Tests/DockTests.cs", src);
        let got: Vec<(&str, &str, bool)> = l
            .tests
            .iter()
            .map(|t| (t.description.as_str(), t.junit_name.as_str(), t.planned))
            .collect();
        assert_eq!(
            got,
            vec![
                ("When a rig is km out, refuses docking", "RefusesFarRigs", false),
                ("Docks a rig in range", "DocksNearRigs", false),
                ("charges fee per hour", "Charges_fee_per_hour", false),
                ("undocks", "Undocks", true),
                ("refuels", "Refuels", true),
            ]
        );
        assert_eq!(l.tests[1].concerns, vec!["docking"]);
    }

    /// When a test's description carries `[manual]`, it is no claim in any
    /// language, while a bare skip stays planned.
    #[test]
    fn manual_tests_are_not_claims() {
        let rust = "/// Dumps a repo [manual]\n#[test]\n#[ignore]\nfn dump() {}\n#[test]\n#[ignore]\nfn later() {}\n";
        let l = lift("src/a.rs", rust);
        assert_eq!(l.tests.iter().map(|t| t.identifier.as_str()).collect::<Vec<_>>(), vec!["later"]);
        assert!(l.tests[0].planned);
        assert!(lift("a.test.ts", "it.skip('dumps [manual]', () => {});\n").tests.is_empty());
        assert!(lift("test_a.py", "def test_dump():\n    \"\"\"Dumps [manual]\"\"\"\n    pass\n").tests.is_empty());
        let cs = "class T {\n    [Fact(DisplayName = \"Dumps [manual]\")]\n    public void Dump() { }\n}\n";
        assert!(lift("ATests.cs", cs).tests.is_empty());
    }

    /// Identifiers read as lowercase words with acronyms kept.
    #[test]
    fn identifiers_de_snake_into_words() {
        assert_eq!(de_snake("keeps_draft_when_save_fails"), "keeps draft when save fails");
        assert_eq!(de_snake("test_keeps_draft"), "keeps draft");
        assert_eq!(de_snake("KeepsDraftWhenSaveFails"), "keeps draft when save fails");
        assert_eq!(de_snake("ParsesJSONReport"), "parses JSON report");
        assert_eq!(de_snake("TestRefusesHTTP"), "refuses HTTP");
    }

    /// Only trailing bracketed tags split off; inner brackets stay.
    #[test]
    fn trailing_tags_split_into_concerns() {
        assert_eq!(
            split_concerns("reads [a] list [io] [session:ab12]"),
            ("reads [a] list".to_string(), vec!["io".to_string(), "session:ab12".to_string()])
        );
        assert_eq!(split_concerns("plain"), ("plain".to_string(), vec![]));
    }
}
