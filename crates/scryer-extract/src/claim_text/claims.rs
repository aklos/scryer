//! Behaviour tests read as claims, and modules read as descriptions.
//!
//! A test's description is its claim: the `///` doc on a Rust `#[test]` fn,
//! the `it("…")` string in Vitest/Jest, a pytest function's docstring, a .NET
//! test's `DisplayName` or XML doc. With none of those, the identifier
//! de-snaked stands in. A todo or skipped test is a planned claim. The module
//! doc (`//!`, a file-head JSDoc, a module docstring) is the description of
//! the node the file belongs to and carries no status.

use crate::code_map as lang;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
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
    /// Every identifier the test's code names (three characters or longer),
    /// read once so the symbols it exercises are a set lookup.
    pub names: BTreeSet<String>,
}

/// Everything one file says about claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLift {
    pub tests: Vec<LiftedTest>,
    pub module_description: Option<String>,
    /// The module doc after its first paragraph: the decisions a later
    /// reader needs, as markdown.
    pub module_body: Option<String>,
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
            (lift.module_description, lift.module_body) = rust_module_doc(root, bytes);
        }
        "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            js_tests(root, bytes, &mut lift.tests);
            (lift.module_description, lift.module_body) = js_module_doc(root, bytes);
        }
        "py" => {
            py_tests(root, bytes, false, &mut lift.tests);
            (lift.module_description, lift.module_body) = py_module_doc(root, bytes);
        }
        "cs" | "go" | "java" | "kt" | "kts" | "rb" | "php" => {
            match ext {
                "cs" => cs_tests(root, bytes, &mut lift.tests),
                "go" => go_tests(root, bytes, &mut lift.tests),
                "java" => java_tests(root, bytes, &mut lift.tests),
                "kt" | "kts" => kotlin_tests(root, bytes, &mut lift.tests),
                "rb" => rb_tests(root, bytes, &mut lift.tests),
                _ => php_tests(root, bytes, &mut lift.tests),
            }
            (lift.module_description, lift.module_body) = head_doc_parts(ext, source);
        }
        _ => (lift.module_description, lift.module_body) = head_doc_parts(ext, source),
    }
    // A `[manual]` test is a tool run by hand, not a claim.
    lift.tests.retain(|t| !t.concerns.iter().any(|c| c == MANUAL_TAG));
    lift.tests.sort_by_key(|t| t.line);
    lift
}

/// The module doc of a language whose file doc is its head comment or, in
/// Clojure, its namespace docstring.
/// Javadoc's `@` tags, and all after them, are no part of the prose.
fn head_doc_parts(ext: &str, source: &str) -> (Option<String>, Option<String>) {
    let Some(doc) = super::head_doc::head_doc(ext, source) else { return (None, None) };
    let prose: Vec<String> = doc.lines.into_iter().take_while(|l| !l.starts_with('@')).collect();
    doc_parts(&prose)
}

/// The tag that keeps a test out of the claims: an ad-hoc tool, run by hand.
pub const MANUAL_TAG: &str = "manual";

/// Split trailing `[tag]`s off a description: `keeps the draft [persistence]`
/// → (`keeps the draft`, `["persistence"]`).
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

/// A claim read as a sentence: its first letter uppercased when its first
/// word is a plain lowercase word (`returns one segment` → `Returns one
/// segment`). A first word that looks like code — backticked, or holding
/// `_`, `.`, `(`, `:`, `/`, a digit or an inner capital (`getModel`, `iOS`)
/// — stays as written.
pub fn sentence_case(text: &str) -> String {
    let first = text.split_whitespace().next().unwrap_or_default();
    let word = first.trim_end_matches([',', ';', ':']);
    let plain = word.starts_with(|c: char| c.is_lowercase())
        && word.chars().all(|c| c.is_lowercase() || c == '\'' || c == '-');
    if !plain {
        return text.to_string();
    }
    let start = text.len() - text.trim_start().len();
    let mut chars = text[start..].chars();
    let head = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    format!("{}{head}{}", &text[..start], chars.as_str())
}

/// An identifier read as words: `keeps_draft_when_save_fails`,
/// `KeepsDraftWhenSaveFails` and `test_keeps_draft` all become
/// `keeps draft …`. Acronyms keep their case.
pub fn de_snake(identifier: &str) -> String {
    let trimmed = identifier
        .strip_prefix("test_")
        .or_else(|| identifier.strip_prefix("Test").filter(|r| r.starts_with(|c: char| c.is_uppercase() || c == '_')))
        .or_else(|| identifier.strip_prefix("test").filter(|r| r.starts_with(char::is_uppercase)))
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
/// A module doc's first paragraph and the rest after it, the rest dedented
/// as one block so its markdown keeps its shape.
fn doc_parts(lines: &[String]) -> (Option<String>, Option<String>) {
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());
    let end = lines[start..].iter().position(|l| l.trim().is_empty()).map_or(lines.len(), |n| start + n);
    let rest: Vec<&str> = lines[end..].iter().map(|l| l.trim_end()).collect();
    let a = rest.iter().position(|l| !l.is_empty()).unwrap_or(rest.len());
    let b = rest.iter().rposition(|l| !l.is_empty()).map_or(a, |i| i + 1);
    let indent = rest[a..b].iter().filter(|l| !l.is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    let body = rest[a..b].iter().map(|l| l.get(indent..).unwrap_or("")).collect::<Vec<_>>().join("\n");
    (first_paragraph(lines), (!body.is_empty()).then_some(body))
}

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
    bytes: &[u8],
    identifier: String,
    doc: Option<String>,
    junit_name: String,
    planned: bool,
) -> LiftedTest {
    let (line, end_line) = line_of(node);
    let documented = doc.is_some();
    let names = body_names(node, bytes, &identifier);
    let raw = doc.unwrap_or_else(|| de_snake(&identifier));
    let (description, concerns) = split_concerns(&raw);
    let description = sentence_case(&description);
    LiftedTest { line, end_line, identifier, description, concerns, junit_name, planned, documented, names }
}

/// The identifiers a test's code spells, three characters or longer, the
/// test's own name left out: what it can be said to exercise. Strings and
/// comments name nothing, and neither does a bare name the test binds itself
/// (`let dir = …`, a parameter): its uses mean the local. A field or method
/// (`r.dir()`) still names what it reaches.
fn body_names(node: Node, bytes: &[u8], own: &str) -> BTreeSet<String> {
    let mut bare = BTreeSet::new();
    let mut members = BTreeSet::new();
    let mut bound = BTreeSet::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        // A path (`super::add`, `a.b.C`) is read leaf by leaf.
        if n.kind().ends_with("identifier") && n.named_child_count() == 0 {
            let t = text(n, bytes);
            if t.len() >= 3 && t != own {
                let set = if n.kind() == "identifier" { &mut bare } else { &mut members };
                set.insert(t.to_string());
            }
            continue;
        }
        if n.kind().contains("comment") {
            continue;
        }
        let binding = match n.kind() {
            "let_declaration" | "for_expression" | "parameter" | "required_parameter" | "optional_parameter" => {
                n.child_by_field_name("pattern")
            }
            "variable_declarator" => n.child_by_field_name("name"),
            "assignment" | "for_statement" | "for_in_statement" => n.child_by_field_name("left"),
            "closure_parameters" | "formal_parameters" | "parameters" => Some(n),
            "arrow_function" => n.child_by_field_name("parameter"),
            _ => None,
        };
        if let Some(b) = binding {
            let mut pending = vec![b];
            while let Some(p) = pending.pop() {
                if matches!(p.kind(), "identifier" | "shorthand_property_identifier_pattern") {
                    bound.insert(text(p, bytes).to_string());
                } else if !p.kind().ends_with("type") && p.kind() != "type_annotation" {
                    let mut cursor = p.walk();
                    pending.extend(p.named_children(&mut cursor));
                }
            }
        }
        let mut cursor = n.walk();
        stack.extend(n.named_children(&mut cursor));
    }
    bare.retain(|name| !bound.contains(name));
    bare.append(&mut members);
    bare
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
                lift.tests.push(make_test(child, bytes, name.clone(), doc, name, planned));
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
fn rust_module_doc(root: Node, bytes: &[u8]) -> (Option<String>, Option<String>) {
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
    doc_parts(&lines)
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
            Some(js_unescape(&t[1..t.len().saturating_sub(1)]))
        }
        _ => None,
    }
}

/// A literal's text as the runner sees it: `\'` → `'`, `\\` → `\`.
fn js_unescape(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some(n @ ('\\' | '\'' | '"' | '`' | '$'))) => {
                out.push(n);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// One step of a walk that tracks the `describe`s around it ([`js_tests`], [`rb_tests`]).
enum SuiteStep<'t> {
    /// Look for tests under a node, inside these `describe`s.
    Walk(Node<'t>, Rc<Vec<String>>, bool),
    /// A test found, emitted when the walk reaches it in source order.
    Emit(LiftedTest),
}

fn js_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    // An explicit stack, not recursion: generated files nest thousands of
    // levels deep (a long chain of joined strings), past a worker thread's
    // stack. Children go on in reverse so tests come out in source order.
    let mut stack = vec![SuiteStep::Walk(root, Rc::new(Vec::new()), false)];
    while let Some(step) = stack.pop() {
        let (node, suite, skipped) = match step {
            SuiteStep::Emit(test) => {
                out.push(test);
                continue;
            }
            SuiteStep::Walk(node, suite, skipped) => (node, suite, skipped),
        };
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            if child.kind() == "call_expression" {
                let shape = child
                    .child_by_field_name("function")
                    .and_then(|f| js_call_shape(f, bytes));
                let args = child.child_by_field_name("arguments");
                let name_node = args.and_then(|a| a.named_child(0));
                let name = name_node.and_then(|n| js_string(n, bytes));
                if let (Some(shape), Some(name)) = (shape, name) {
                    let skip_here = shape.modifiers.iter().any(|m| matches!(*m, "skip" | "todo"))
                        || shape.base.starts_with('x');
                    let base = shape.base.trim_start_matches(['x', 'f']);
                    match base {
                        "describe" | "suite" | "context" => {
                            if let Some(args) = args {
                                let mut inner = (*suite).clone();
                                inner.push(name);
                                stack.push(SuiteStep::Walk(args, Rc::new(inner), skipped || skip_here));
                            }
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
                            // The identifier is the literal as written: anchors
                            // find the test block by it.
                            let raw = name_node.map(|n| {
                                let t = text(n, bytes);
                                t[1..t.len().saturating_sub(1)].to_string()
                            });
                            let identifier = raw.unwrap_or_else(|| name.clone());
                            stack.push(SuiteStep::Emit(make_test(child, bytes, identifier, Some(name), junit, planned)));
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            stack.push(SuiteStep::Walk(child, Rc::clone(&suite), skipped));
        }
    }
}

// --- Go, JUnit/TestNG, RSpec/Minitest, PHPUnit ---

/// Every node under `root` in source order, walked without recursion.
fn descendants(root: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        out.push(n);
        let mut cursor = n.walk();
        let children: Vec<Node> = n.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// The prose of the comments right above `node`: a run of comment siblings
/// ending on the line before it, markers gone.
fn comment_above(node: Node, bytes: &[u8], kinds: &[&str]) -> Vec<String> {
    // A grammar may wrap a body's statements in a node that starts with the
    // first of them, leaving a comment above it outside (Ruby's class body).
    let mut anchor = node;
    while anchor.prev_named_sibling().is_none() {
        match anchor.parent() {
            Some(p) if p.start_position().row == anchor.start_position().row => anchor = p,
            _ => break,
        }
    }
    let mut raws = Vec::new();
    let mut row = node.start_position().row;
    let mut cur = anchor.prev_named_sibling();
    while let Some(c) = cur {
        if !kinds.contains(&c.kind()) || c.end_position().row + 1 != row {
            break;
        }
        raws.push(text(c, bytes));
        row = c.start_position().row;
        cur = c.prev_named_sibling();
    }
    raws.reverse();
    raws.iter()
        .flat_map(|raw| raw.lines())
        .map(|l| {
            let l = l.trim().trim_start_matches("/**").trim_start_matches("/*").trim_end_matches("*/").trim();
            let l = l.strip_prefix("//").or_else(|| l.strip_prefix('#')).or_else(|| l.strip_prefix('*')).unwrap_or(l);
            l.trim().to_string()
        })
        .collect()
}

/// The first statement of a body, through the list nodes grammars wrap
/// statements in.
fn first_statement(body: Node) -> Option<Node> {
    let mut n = body;
    loop {
        let mut cursor = n.walk();
        let first = n.named_children(&mut cursor).find(|c| !c.kind().contains("comment"))?;
        if matches!(first.kind(), "statement_list" | "body_statement" | "compound_statement" | "block") {
            n = first;
        } else {
            return Some(first);
        }
    }
}

/// A string literal's text without its quotes.
fn quoted(node: Node, bytes: &[u8]) -> Option<String> {
    let t = text(node, bytes).trim();
    let q = t.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    Some(t.trim_start_matches(q).trim_end_matches(q).to_string())
}

/// `func TestX(t *testing.T)`: its doc comment (which opens with the name it
/// documents), else its name; todo while it opens with `t.Skip`.
fn go_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    let mut cursor = root.walk();
    for f in root.named_children(&mut cursor).filter(|n| n.kind() == "function_declaration") {
        let Some(name) = f.child_by_field_name("name").map(|n| text(n, bytes).to_string()) else { continue };
        let params = f.child_by_field_name("parameters").map_or("", |p| text(p, bytes));
        if !name.starts_with("Test") || !params.contains("testing.T") {
            continue;
        }
        let doc = first_paragraph(&comment_above(f, bytes, &["comment"])).map(|d| {
            d.strip_prefix(name.as_str()).map(str::trim_start).filter(|r| !r.is_empty()).map_or(d.clone(), str::to_string)
        });
        let planned = f
            .child_by_field_name("body")
            .and_then(first_statement)
            .is_some_and(|s| text(s, bytes).split_once('.').is_some_and(|(_, call)| call.starts_with("Skip")));
        out.push(make_test(f, bytes, name.clone(), doc, name, planned));
    }
}

const JAVA_TEST_ANNOTATIONS: &[&str] = &["Test", "ParameterizedTest", "RepeatedTest", "TestFactory", "TestTemplate"];

/// A JUnit or TestNG test method: its `@DisplayName`, else its Javadoc, else
/// its name; todo while `@Disabled` or `@Ignore`d.
fn java_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    for m in descendants(root).into_iter().filter(|n| n.kind() == "method_declaration") {
        let mut cursor = m.walk();
        let annotations: Vec<(String, Node)> = m
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "modifiers")
            .flat_map(|mods| {
                let mut c = mods.walk();
                mods.named_children(&mut c).filter(|a| a.kind().ends_with("annotation")).collect::<Vec<_>>()
            })
            .filter_map(|a| {
                let name = text(a.child_by_field_name("name")?, bytes);
                Some((name.rsplit('.').next().unwrap_or(name).to_string(), a))
            })
            .collect();
        let has = |names: &[&str]| annotations.iter().any(|(n, _)| names.contains(&n.as_str()));
        if !has(JAVA_TEST_ANNOTATIONS) {
            continue;
        }
        let Some(name) = m.child_by_field_name("name").map(|n| text(n, bytes).to_string()) else { continue };
        let display = annotations
            .iter()
            .find(|(n, _)| n == "DisplayName")
            .and_then(|(_, a)| a.child_by_field_name("arguments"))
            .and_then(|args| args.named_child(0))
            .and_then(|lit| quoted(lit, bytes));
        let javadoc = || {
            let lines: Vec<String> =
                comment_above(m, bytes, &["block_comment", "line_comment"]).into_iter().take_while(|l| !l.starts_with('@')).collect();
            first_paragraph(&lines)
        };
        let doc = display.or_else(javadoc);
        out.push(make_test(m, bytes, name.clone(), doc, name, has(&["Disabled", "Ignore"])));
    }
}

/// A Kotlin annotation's name and its first string argument:
/// `@Test` → (`Test`, None), `@DisplayName("…")` → (`DisplayName`, "…").
pub(crate) fn kotlin_annotation<'t>(a: Node<'t>, bytes: &[u8]) -> Option<(String, Option<Node<'t>>)> {
    let user_type = descendants(a).into_iter().find(|n| n.kind() == "user_type")?;
    let name = text(user_type, bytes);
    let arg = descendants(a).into_iter().find(|n| n.kind() == "string_literal");
    Some((name.rsplit('.').next().unwrap_or(name).to_string(), arg))
}

/// A Kotlin JUnit test: its `@DisplayName`, else its KDoc, else its name
/// (often a backticked sentence); todo while `@Disabled` or `@Ignore`d.
fn kotlin_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    for f in descendants(root).into_iter().filter(|n| n.kind() == "function_declaration") {
        let mut cursor = f.walk();
        let annotations: Vec<(String, Option<Node>)> = f
            .named_children(&mut cursor)
            .filter(|n| n.kind() == "modifiers")
            .flat_map(|mods| {
                let mut c = mods.walk();
                mods.named_children(&mut c).filter(|a| a.kind() == "annotation").collect::<Vec<_>>()
            })
            .filter_map(|a| kotlin_annotation(a, bytes))
            .collect();
        let has = |names: &[&str]| annotations.iter().any(|(n, _)| names.contains(&n.as_str()));
        if !has(JAVA_TEST_ANNOTATIONS) {
            continue;
        }
        let Some(name) = f.child_by_field_name("name").map(|n| text(n, bytes).trim_matches('`').to_string()) else { continue };
        let display = annotations.iter().find(|(n, _)| n == "DisplayName").and_then(|(_, arg)| *arg).and_then(|lit| quoted(lit, bytes));
        let kdoc = || {
            let lines: Vec<String> = comment_above(f, bytes, &["block_comment", "line_comment"]).into_iter().take_while(|l| !l.starts_with('@')).collect();
            first_paragraph(&lines)
        };
        let doc = display.or_else(kdoc);
        out.push(make_test(f, bytes, name.clone(), doc, name, has(&["Disabled", "Ignore"])));
    }
}

/// RSpec examples inside their `describe`s, and Minitest's `test_` methods
/// and `test "…"` blocks. An RSpec example's report name is its full
/// description: the `describe`s and the example, joined by spaces.
fn rb_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    let skips = |block: Option<Node>| {
        block
            .and_then(|b| b.child_by_field_name("body").or(Some(b)))
            .and_then(first_statement)
            .is_some_and(|s| text(s, bytes).split(|c: char| !c.is_alphanumeric() && c != '_').next() == Some("skip"))
    };
    let mut stack = vec![SuiteStep::Walk(root, Rc::new(Vec::new()), false)];
    while let Some(step) = stack.pop() {
        let (node, suite, skipped) = match step {
            SuiteStep::Emit(test) => {
                out.push(test);
                continue;
            }
            SuiteStep::Walk(node, suite, skipped) => (node, suite, skipped),
        };
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            if child.kind() == "method" {
                let name = child.child_by_field_name("name").map(|n| text(n, bytes).to_string()).unwrap_or_default();
                if name.starts_with("test_") {
                    let doc = first_paragraph(&comment_above(child, bytes, &["comment"]));
                    let planned = skipped || skips(Some(child));
                    stack.push(SuiteStep::Emit(make_test(child, bytes, name.clone(), doc, name, planned)));
                    continue;
                }
            }
            if child.kind() == "call" {
                let method = child.child_by_field_name("method").map_or("", |m| text(m, bytes));
                let arg = child.child_by_field_name("arguments").and_then(|a| a.named_child(0));
                let label = arg.and_then(|a| if a.kind() == "string" { quoted(a, bytes) } else { None });
                let block = child.child_by_field_name("block");
                match method {
                    "describe" | "context" | "feature" | "xdescribe" | "xcontext" | "fdescribe" | "fcontext" => {
                        let name = label.or_else(|| arg.map(|a| text(a, bytes).to_string()));
                        if let (Some(name), Some(block)) = (name, block) {
                            let mut inner = (*suite).clone();
                            inner.push(name);
                            stack.push(SuiteStep::Walk(block, Rc::new(inner), skipped || method.starts_with('x')));
                            continue;
                        }
                    }
                    "it" | "specify" | "scenario" | "example" | "fit" | "xit" | "xspecify" | "pending" | "skip" => {
                        if let Some(label) = label.filter(|_| !suite.is_empty()) {
                            let planned = skipped || block.is_none() || matches!(method, "xit" | "xspecify" | "pending" | "skip");
                            let junit = suite.iter().cloned().chain(std::iter::once(label.clone())).collect::<Vec<_>>().join(" ");
                            stack.push(SuiteStep::Emit(make_test(child, bytes, label.clone(), Some(label), junit, planned)));
                            continue;
                        }
                    }
                    "test" => {
                        if let Some(label) = label {
                            let identifier = format!("test_{}", label.split_whitespace().collect::<Vec<_>>().join("_"));
                            let planned = skipped || block.is_none() || skips(block);
                            stack.push(SuiteStep::Emit(make_test(child, bytes, identifier.clone(), Some(label), identifier, planned)));
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            stack.push(SuiteStep::Walk(child, Rc::clone(&suite), skipped));
        }
    }
}

/// A PHPUnit test method (`test…`, `@test` or `#[Test]`): its TestDox, else
/// its name; todo while it only marks itself incomplete or skipped.
fn php_tests(root: Node, bytes: &[u8], out: &mut Vec<LiftedTest>) {
    for m in descendants(root).into_iter().filter(|n| n.kind() == "method_declaration") {
        let Some(name) = m.child_by_field_name("name").map(|n| text(n, bytes).to_string()) else { continue };
        let docblock = comment_above(m, bytes, &["comment"]);
        let attributes: Vec<(String, Option<String>)> = m
            .child_by_field_name("attributes")
            .map(|list| {
                descendants(list)
                    .into_iter()
                    .filter(|n| n.kind() == "attribute")
                    .map(|a| {
                        let name = a.named_child(0).map_or("", |n| text(n, bytes));
                        let arg = descendants(a).into_iter().find(|n| n.kind() == "string").and_then(|s| quoted(s, bytes));
                        (name.rsplit('\\').next().unwrap_or(name).to_string(), arg)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let is_test = name.starts_with("test") || docblock.iter().any(|l| l == "@test") || attributes.iter().any(|(n, _)| n == "Test");
        if !is_test {
            continue;
        }
        let testdox = attributes
            .iter()
            .find(|(n, _)| n == "TestDox")
            .and_then(|(_, arg)| arg.clone())
            .or_else(|| docblock.iter().find_map(|l| l.strip_prefix("@testdox ").map(|r| r.trim().to_string())));
        let planned = m
            .child_by_field_name("body")
            .and_then(first_statement)
            .is_some_and(|s| ["markTestIncomplete", "markTestSkipped"].iter().any(|c| text(s, bytes).contains(c)));
        out.push(make_test(m, bytes, name.clone(), testdox, name, planned));
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

/// The byte span of the file-head JSDoc [`js_module_doc`] reads, if any.
pub(crate) fn js_head_doc_span(root: Node, source: &str) -> Option<(usize, usize)> {
    js_head_doc(root, source.as_bytes()).map(|n| (n.start_byte(), n.end_byte()))
}

/// A JSDoc at the head of the file that documents the file, not the first
/// declaration under it.
fn js_module_doc(root: Node, bytes: &[u8]) -> (Option<String>, Option<String>) {
    js_head_doc(root, bytes).map_or((None, None), |n| doc_parts(&jsdoc_lines(text(n, bytes))))
}

fn js_head_doc<'t>(root: Node<'t>, bytes: &[u8]) -> Option<Node<'t>> {
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
        return Some(*child);
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
                out.push(make_test(child, bytes, name.clone(), doc, name, skipped || stub));
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

fn py_module_doc(root: Node, bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Some(doc) = py_docstring(root, bytes) else { return (None, None) };
    doc_parts(&doc.lines().map(str::to_string).collect::<Vec<_>>())
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
        out.push(make_test(child, bytes, name.clone(), doc, name, skipped || stub));
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

pub(crate) fn strip_xml_tags(s: &str) -> String {
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

    const GO_SRC: &str = "package cargo\n\nimport \"testing\"\n\n// TestRejectsEmpty rejects an empty rack.\nfunc TestRejectsEmpty(t *testing.T) {\n\tif false {\n\t\tt.Fatal()\n\t}\n}\n\nfunc TestLoadsTheHold(t *testing.T) {\n\tt.Skip(\"todo\")\n}\n\nfunc helper(t *testing.T) {}\n\nfunc TestMain(m *testing.M) {}\n";
    const JAVA_SRC: &str = "class RackTest {\n  @Test\n  @DisplayName(\"Rejects an empty rack\")\n  void rejectsEmpty() {}\n\n  /** Loads the hold first. @see Hold */\n  @org.junit.jupiter.api.Test\n  void loadsHold() {}\n\n  @Test\n  @Disabled\n  void stacksCrates() {}\n\n  void helper() {}\n}\n";
    const RSPEC_SRC: &str = "RSpec.describe Invoice do\n  context \"when paid\" do\n    it \"totals the lines\" do\n      expect(1).to eq 1\n    end\n    xit \"rounds up\" do\n    end\n    it \"refunds the change\"\n  end\n  it { is_expected.to be_valid }\nend\n";
    const MINITEST_SRC: &str = "class InvoiceTest < Minitest::Test\n  # Totals the lines.\n  def test_totals\n    assert true\n  end\n\n  def test_rounds_up\n    skip \"later\"\n  end\n\n  test \"refunds the change\" do\n    assert true\n  end\n\n  def helper\n  end\nend\n";
    const PHP_SRC: &str = "<?php\nclass RackTest extends TestCase {\n  /**\n   * @testdox Rejects an empty rack\n   */\n  public function testRejectsEmpty(): void { $this->assertTrue(true); }\n\n  #[TestDox('Loads the hold')]\n  #[Test]\n  public function loads(): void { $this->assertTrue(true); }\n\n  public function testStacksCrates(): void { $this->markTestIncomplete('todo'); }\n\n  private function helper(): void {}\n}\n";

    const KOTLIN_SRC: &str = "package space.cargo\n\nclass RackTest {\n    @Test\n    @DisplayName(\"Rejects an empty rack\")\n    fun rejectsEmpty() {}\n\n    /** Loads the hold first. */\n    @org.junit.jupiter.api.Test\n    fun loadsHold() {}\n\n    @Test\n    @Disabled(\"todo\")\n    fun `stacks crates in order`() {}\n\n    @Test fun `keeps the manifest`() {}\n\n    fun helper() {}\n}\n";

    fn lift(path: &str, src: &str) -> FileLift {
        lift_file(path, src, &mut Parser::new())
    }

    /// When a C#, Java, Kotlin, Go, C, C++, PHP or Ruby file opens with a comment
    /// that is not a licence and not on a declaration, reads it as the
    /// file's module doc.
    #[test]
    fn reads_head_comments_as_module_docs() {
        let desc = |path: &str, src: &str| lift(path, src).module_description;
        let cs = lift("Rack.cs", "// Moves cargo between racks.\n//\n// Racks before holds.\n\nnamespace Shared;\npublic class Rack {}\n");
        assert_eq!(cs.module_description.as_deref(), Some("Moves cargo between racks."));
        assert_eq!(cs.module_body.as_deref(), Some("Racks before holds."));
        assert_eq!(
            desc("Cab.cs", "// Copyright 2026 Truckers. All rights reserved.\n\n// The cab's screens.\n\nusing System;\n").as_deref(),
            Some("The cab's screens.")
        );
        assert_eq!(desc("Head.cs", "/// <summary>Racks and holds.</summary>\nusing System;\n").as_deref(), Some("Racks and holds."));
        assert_eq!(desc("Rack.cs", "/// <summary>A rack.</summary>\npublic class Rack {}\n"), None, "a type's own doc");
        assert_eq!(desc("cargo.go", "// Package cargo moves cargo.\npackage cargo\n").as_deref(), Some("Package cargo moves cargo."));
        assert_eq!(desc("Rack.kt", "// Racks cargo.\npackage space.cargo\n\nclass Rack\n").as_deref(), Some("Racks cargo."));
        assert_eq!(
            desc("package-info.java", "/**\n * Market rules.\n *\n * @author someone\n */\npackage market;\n").as_deref(),
            Some("Market rules.")
        );
        assert_eq!(lift("package-info.java", "/**\n * Market rules.\n *\n * @author someone\n */\npackage market;\n").module_body, None);
        assert_eq!(desc("ring.c", "/* Ring buffer for samples. */\n#include <stdio.h>\n").as_deref(), Some("Ring buffer for samples."));
        assert_eq!(desc("ring.hpp", "// Ring buffer for samples.\n#pragma once\n").as_deref(), Some("Ring buffer for samples."));
        assert_eq!(desc("Router.php", "<?php\n// Routes requests.\n\nnamespace App;\n").as_deref(), Some("Routes requests."));
        assert_eq!(
            desc("billing.rb", "# frozen_string_literal: true\n\n# Bills customers.\nmodule Billing\nend\n").as_deref(),
            Some("Bills customers.")
        );
        assert_eq!(desc("invoice.rb", "# An invoice.\nclass Invoice\nend\n"), None, "a class's own doc");
    }

    /// Reads a Clojure namespace's docstring as its module doc.
    #[test]
    fn reads_a_clojure_ns_docstring() {
        let l = lift("cargo.clj", "(ns space.cargo\n  \"Moves cargo.\n\n  Racks first.\"\n  (:require [clojure.string]))\n");
        assert_eq!(l.module_description.as_deref(), Some("Moves cargo."));
        assert_eq!(l.module_body.as_deref(), Some("Racks first."));
        assert_eq!(lift("bare.clj", "(ns space.bare\n  (:require [clojure.string]))\n").module_description, None);
    }

    /// Reads a Go test function as a claim worded by the comment above it,
    /// falling back to its name.
    #[test]
    fn lifts_go_tests() {
        let rows = |l: FileLift| -> Vec<(String, String)> {
            l.tests.into_iter().map(|t| (t.description, t.junit_name)).collect()
        };
        let r = |d: &str, j: &str, _todo: bool| (d.to_string(), j.to_string());
        assert_eq!(
            rows(lift("cargo_test.go", GO_SRC)),
            vec![r("Rejects an empty rack.", "TestRejectsEmpty", false), r("Loads the hold", "TestLoadsTheHold", true)]
        );
    }

    /// While a Go test opens with `t.Skip`, reads its claim as todo.
    #[test]
    fn go_tests_that_skip_are_todo() {
        let planned: Vec<bool> = lift("cargo_test.go", GO_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, true]);
    }

    /// Reads a JUnit or TestNG test method as a claim worded by the first of
    /// its `@DisplayName`, its Javadoc and its name.
    #[test]
    fn lifts_java_tests() {
        let rows = |l: FileLift| -> Vec<(String, String)> {
            l.tests.into_iter().map(|t| (t.description, t.junit_name)).collect()
        };
        let r = |d: &str, j: &str, _todo: bool| (d.to_string(), j.to_string());
        assert_eq!(
            rows(lift("RackTest.java", JAVA_SRC)),
            vec![
                r("Rejects an empty rack", "rejectsEmpty", false),
                r("Loads the hold first. @see Hold", "loadsHold", false),
                r("Stacks crates", "stacksCrates", true),
            ]
        );
    }

    /// While a JUnit test is `@Disabled` or `@Ignore`d, reads its claim as
    /// todo.
    #[test]
    fn disabled_java_tests_are_todo() {
        let planned: Vec<bool> = lift("RackTest.java", JAVA_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, false, true]);
    }

    /// Reads an RSpec example as a claim worded by its `it` string, named
    /// in reports with its `describe`s.
    #[test]
    fn lifts_rspec_examples() {
        let rows = |l: FileLift| -> Vec<(String, String)> {
            l.tests.into_iter().map(|t| (t.description, t.junit_name)).collect()
        };
        let r = |d: &str, j: &str, _todo: bool| (d.to_string(), j.to_string());
        assert_eq!(
            rows(lift("invoice_spec.rb", RSPEC_SRC)),
            vec![
                r("Totals the lines", "Invoice when paid totals the lines", false),
                r("Rounds up", "Invoice when paid rounds up", true),
                r("Refunds the change", "Invoice when paid refunds the change", true),
            ]
        );
    }

    /// If an RSpec example is `xit`, `pending`, `skip` or has no block, then
    /// reads its claim as todo.
    #[test]
    fn pending_rspec_examples_are_todo() {
        let planned: Vec<bool> = lift("invoice_spec.rb", RSPEC_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, true, true]);
    }

    /// Reads a Minitest `test_` method or `test "…"` block as a claim.
    #[test]
    fn lifts_minitest_tests() {
        let rows = |l: FileLift| -> Vec<(String, String)> {
            l.tests.into_iter().map(|t| (t.description, t.junit_name)).collect()
        };
        let r = |d: &str, j: &str, _todo: bool| (d.to_string(), j.to_string());
        assert_eq!(
            rows(lift("invoice_test.rb", MINITEST_SRC)),
            vec![
                r("Totals the lines.", "test_totals", false),
                r("Rounds up", "test_rounds_up", true),
                r("Refunds the change", "test_refunds_the_change", false),
            ]
        );
    }

    /// While a Minitest test opens with `skip`, reads its claim as todo.
    #[test]
    fn skipped_minitests_are_todo() {
        let planned: Vec<bool> = lift("invoice_test.rb", MINITEST_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, true, false]);
    }

    /// Reads a PHPUnit test method as a claim worded by its TestDox, falling
    /// back to its name.
    #[test]
    fn lifts_phpunit_tests() {
        let rows = |l: FileLift| -> Vec<(String, String)> {
            l.tests.into_iter().map(|t| (t.description, t.junit_name)).collect()
        };
        let r = |d: &str, j: &str, _todo: bool| (d.to_string(), j.to_string());
        assert_eq!(
            rows(lift("RackTest.php", PHP_SRC)),
            vec![
                r("Rejects an empty rack", "testRejectsEmpty", false),
                r("Loads the hold", "loads", false),
                r("Stacks crates", "testStacksCrates", true),
            ]
        );
    }

    /// While a PHPUnit test only marks itself incomplete or skipped, reads
    /// its claim as todo.
    #[test]
    fn incomplete_phpunit_tests_are_todo() {
        let planned: Vec<bool> = lift("RackTest.php", PHP_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, false, true]);
    }

    /// Reads a Kotlin JUnit test as a claim worded by the first of its
    /// `@DisplayName`, its KDoc and its name, backticks dropped.
    #[test]
    fn lifts_kotlin_tests() {
        let got: Vec<(String, String)> = lift("RackTest.kt", KOTLIN_SRC).tests.into_iter().map(|t| (t.description, t.junit_name)).collect();
        let r = |d: &str, j: &str| (d.to_string(), j.to_string());
        assert_eq!(
            got,
            vec![
                r("Rejects an empty rack", "rejectsEmpty"),
                r("Loads the hold first.", "loadsHold"),
                r("Stacks crates in order", "stacks crates in order"),
                r("Keeps the manifest", "keeps the manifest"),
            ]
        );
    }

    /// While a Kotlin test is `@Disabled` or `@Ignore`d, reads its claim as
    /// todo.
    #[test]
    fn disabled_kotlin_tests_are_todo() {
        let planned: Vec<bool> = lift("RackTest.kt", KOTLIN_SRC).tests.iter().map(|t| t.planned).collect();
        assert_eq!(planned, vec![false, false, true, false]);
    }

    /// When a JS file nests thousands of expressions deep, such as a long
    /// chain of joined strings, reads its tests without overflowing a
    /// worker thread's stack.
    #[test]
    fn reads_a_deeply_nested_js_file_on_a_small_stack() {
        let src = format!("const s = {}'a';\ndescribe(\"pack\", () => {{ it(\"loads\", () => {{}}); }});\n", "'a' + ".repeat(50_000));
        let tests = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || lift("textures.js", &src).tests)
            .unwrap()
            .join()
            .unwrap();
        let got: Vec<&str> = tests.iter().map(|t| t.junit_name.as_str()).collect();
        assert_eq!(got, vec!["pack > loads"]);
    }

    /// When a claim's first word is a plain lowercase word it starts with a capital, and when that word looks like code it stays as written, while the JUnit name keeps the report's spelling.
    #[test]
    fn claims_read_as_sentences() {
        let src = "it(\"returns one plain segment\", () => {});\nit(\"getModel reads the model\", () => {});\nit(\"`save` keeps it\", () => {});\nit(\"iOS keeps it\", () => {});\nit(\"read_model answers\", () => {});\nit(\"v2 parses\", () => {});\n";
        let l = lift("a.test.ts", src);
        let got: Vec<(&str, &str)> = l.tests.iter().map(|t| (t.description.as_str(), t.junit_name.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("Returns one plain segment", "returns one plain segment"),
                ("getModel reads the model", "getModel reads the model"),
                ("`save` keeps it", "`save` keeps it"),
                ("iOS keeps it", "iOS keeps it"),
                ("read_model answers", "read_model answers"),
                ("v2 parses", "v2 parses"),
            ]
        );
        let rs = lift("src/x.rs", "#[test]\nfn retries_after_timeout() {}\n");
        assert_eq!(rs.tests[0].description, "Retries after timeout");
        assert_eq!(rs.tests[0].junit_name, "retries_after_timeout");
        assert_eq!(sentence_case("don't drop it"), "Don't drop it");
        assert_eq!(sentence_case("a.b works"), "a.b works");
    }

    /// Lifts a module doc's first paragraph as its description and the rest
    /// after it as its decisions, dedented, markdown kept.
    #[test]
    fn lifts_the_module_docs_decisions_after_its_description() {
        let rs = lift("src/route.rs", "//! Routes edges.\n//!\n//! Why a grid:\n//!\n//!   - cards stay put\nfn a() {}\n");
        assert_eq!(rs.module_description.as_deref(), Some("Routes edges."));
        assert_eq!(rs.module_body.as_deref(), Some("Why a grid:\n\n  - cards stay put"));
        let ts = lift("route.ts", "/**\n * Routes edges.\n *\n * Why a grid.\n */\n\nexport const a = 1;\n");
        assert_eq!(ts.module_body.as_deref(), Some("Why a grid."));
        let py = lift("route.py", "\"\"\"Routes edges.\n\nWhy a grid.\n\"\"\"\n");
        assert_eq!(py.module_body.as_deref(), Some("Why a grid."));
        assert_eq!(lift("one.rs", "//! Only a summary.\nfn a() {}\n").module_body, None);
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
        assert_eq!(l.tests[1].description, "Retries after timeout");
        assert!(!l.tests[1].documented);
        assert_eq!(l.test_spans.len(), 1, "the cfg(test) module is test code");
    }

    /// If a Rust test is `#[ignore]`d or its body is only `todo!()` or `unimplemented!()`, then it is planned.
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
                ("Hides children", "tree > when collapsed > hides children [ui]", false),
                ("Remembers the fold", "tree > when collapsed > remembers the fold", true),
                ("Renders %s rows", "tree > renders %s rows", false),
                ("Scrolls to selection", "tree > scrolls to selection", true),
                ("Does a thing", "later > does a thing", true),
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
                ("Scales linearly", false),
                ("Applies discount", true),
                ("Refunds the whole amount", true),
            ]
        );
        assert_eq!(l.tests[0].concerns, vec!["money"]);
        assert_eq!(l.tests[1].junit_name, "test_scales_linearly");
    }

    /// When a .NET test is lifted, takes its DisplayName, its XML summary or its de-snaked name as the claim, the first present, and reads a Skip or NotImplementedException body as planned.
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
                ("Charges fee per hour", "Charges_fee_per_hour", false),
                ("Undocks", "Undocks", true),
                ("Refuels", "Refuels", true),
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

