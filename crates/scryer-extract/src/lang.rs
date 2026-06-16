//! Tree-sitter language registry + definition enumeration.
//!
//! Given a source file, [`parse_file`] returns the top-level *definitions* it
//! declares (functions, types, methods, module-level bindings) with their exact
//! line ranges and — for data shapes (structs, enums, interfaces, type-object
//! literals, classes with fields) — their declared field names. It also returns
//! every identifier occurrence, which the model builder resolves into a
//! reference graph. Everything here is grammar-derived: no name tables, no
//! regex heuristics on contents.

use std::path::Path;
use tree_sitter::{Language, Node, Parser};

/// Map a file extension to a bundled tree-sitter grammar, if one exists.
/// Mirrors the inspector's `symbols.rs` so the extractor and inspector agree
/// on which files are parseable.
pub fn language_for_ext(ext: &str) -> Option<Language> {
    let f = match ext {
        "rs" => tree_sitter_rust::LANGUAGE,
        "ts" | "mts" | "cts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX,
        "js" | "jsx" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE,
        "py" | "pyi" => tree_sitter_python::LANGUAGE,
        "go" => tree_sitter_go::LANGUAGE,
        "java" => tree_sitter_java::LANGUAGE,
        "rb" => tree_sitter_ruby::LANGUAGE,
        "c" | "h" => tree_sitter_c::LANGUAGE,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => tree_sitter_cpp::LANGUAGE,
        "cs" => tree_sitter_c_sharp::LANGUAGE,
        "php" => tree_sitter_php::LANGUAGE_PHP,
        _ => return None,
    };
    Some(f.into())
}

pub fn ext_of(path: &Path) -> Option<&str> {
    path.extension()?.to_str()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Rust,
    TsLike,
    Python,
    Generic,
}

fn family_for_ext(ext: &str) -> Family {
    match ext {
        "rs" => Family::Rust,
        "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Family::TsLike,
        "py" | "pyi" => Family::Python,
        _ => Family::Generic,
    }
}

/// One code definition: an addressable symbol.
#[derive(Debug, Clone)]
pub struct Def {
    pub name: String,
    /// 1-based inclusive line range of the whole definition.
    pub start_line: u32,
    pub end_line: u32,
    /// Declared field/variant names when this definition is a data shape
    /// (struct, enum, interface, type-object, class with fields). Empty for
    /// behavior-only definitions.
    pub fields: Vec<String>,
    /// True when the definition declares a data shape, even if `fields` is empty
    /// (e.g. a tuple struct). Drives whether the node carries `properties`.
    pub is_data_shape: bool,
}

/// An identifier occurrence, used to resolve a reference graph between symbols.
#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    /// 1-based line of the occurrence.
    pub line: u32,
}

/// A multi-segment qualified path reference — either a `use` path or a
/// fully-qualified reference at a call/type site (`scryer_extract::anchors::
/// write_baseline`). The resolver maps the head segment to a container (via the
/// crate/package manifest map) and the tail to a symbol, producing the
/// cross-container edges that bare-name resolution cannot. Currently emitted for
/// Rust only; empty for other languages (each needs its own import grammar).
#[derive(Debug, Clone)]
pub struct PathRef {
    /// Path segments head-to-leaf, e.g. `["scryer_extract", "anchors", "write_baseline"]`.
    pub segments: Vec<String>,
    /// 1-based line of the reference: the `use` line, or the call/type site.
    pub line: u32,
}

#[derive(Debug, Clone, Default)]
pub struct FileParse {
    pub defs: Vec<Def>,
    pub idents: Vec<Ident>,
    /// Qualified path references for cross-container resolution. See [`PathRef`].
    pub paths: Vec<PathRef>,
}

/// Parse a source file into its definitions + identifier occurrences. Returns
/// `None` for unsupported extensions or unparseable input.
pub fn parse_file(path: &Path, source: &str) -> Option<FileParse> {
    parse_file_with(path, source, &mut Parser::new())
}

/// Parse with a caller-owned parser. Reusing one parser per worker avoids
/// repeated parser allocation while still switching grammars per file.
pub fn parse_file_with(path: &Path, source: &str, parser: &mut Parser) -> Option<FileParse> {
    let ext = ext_of(path)?;
    let language = language_for_ext(ext)?;
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut defs: Vec<Def> = Vec::new();
    match family_for_ext(ext) {
        Family::Rust => collect_rust(root, bytes, &mut defs),
        Family::TsLike => collect_ts(root, bytes, &mut defs),
        Family::Python => collect_python(root, bytes, &mut defs),
        Family::Generic => collect_generic(root, bytes, &mut defs),
    }

    let mut idents: Vec<Ident> = Vec::new();
    collect_idents(root, bytes, &mut idents);

    // Qualified path references for cross-container resolution. Rust-only for
    // now; the grammar nodes (`use_declaration`, `scoped_identifier`) are
    // Rust-specific. Other families leave `paths` empty.
    let mut paths: Vec<PathRef> = Vec::new();
    if family_for_ext(ext) == Family::Rust {
        collect_use_paths(root, bytes, &mut paths);
        collect_qualified_paths(root, bytes, &mut paths);
    }

    Some(FileParse {
        defs,
        idents,
        paths,
    })
}

// --- shared helpers ---

fn named_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut cur = node.walk();
    node.named_children(&mut cur).collect()
}

fn field_text(node: Node, field: &str, bytes: &[u8]) -> Option<String> {
    node.child_by_field_name(field)?
        .utf8_text(bytes)
        .ok()
        .map(|s| s.to_string())
}

fn line_span(node: Node) -> (u32, u32) {
    (
        node.start_position().row as u32 + 1,
        node.end_position().row as u32 + 1,
    )
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn push_def(
    defs: &mut Vec<Def>,
    name: Option<String>,
    node: Node,
    fields: Vec<String>,
    data: bool,
) {
    if let Some(name) = name {
        if is_identifier(&name) {
            let (start_line, end_line) = line_span(node);
            defs.push(Def {
                name,
                start_line,
                end_line,
                fields,
                is_data_shape: data,
            });
        }
    }
}

// --- Rust ---

fn collect_rust(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    for child in named_children(node) {
        match child.kind() {
            "function_item" | "const_item" | "static_item" | "macro_definition" | "type_item"
            | "trait_item" => {
                push_def(
                    defs,
                    field_text(child, "name", bytes),
                    child,
                    Vec::new(),
                    false,
                );
            }
            "struct_item" => {
                let fields = rust_struct_fields(child, bytes);
                push_def(defs, field_text(child, "name", bytes), child, fields, true);
            }
            "enum_item" => {
                let fields = rust_enum_variants(child, bytes);
                push_def(defs, field_text(child, "name", bytes), child, fields, true);
            }
            // Namespacing items carry no symbol of their own — descend for the
            // definitions inside them (impl methods, module items).
            "impl_item" | "mod_item" => {
                if let Some(body) = child.child_by_field_name("body") {
                    collect_rust(body, bytes, defs);
                }
            }
            _ => {}
        }
    }
}

fn rust_struct_fields(struct_item: Node, bytes: &[u8]) -> Vec<String> {
    let Some(body) = struct_item.child_by_field_name("body") else {
        return Vec::new();
    };
    // Named struct => field_declaration_list of field_declaration(name: field_identifier).
    // Tuple struct => ordered_field_declaration_list (no names).
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "field_declaration")
        .filter_map(|n| field_text(n, "name", bytes))
        .collect()
}

fn rust_enum_variants(enum_item: Node, bytes: &[u8]) -> Vec<String> {
    let Some(body) = enum_item.child_by_field_name("body") else {
        return Vec::new();
    };
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "enum_variant")
        .filter_map(|n| field_text(n, "name", bytes))
        .collect()
}

// --- TypeScript / JavaScript ---

fn collect_ts(root: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    // Module level only: direct children of the program, unwrapping exports.
    // Descending further would pick up locals (every const/let is a
    // variable_declarator) which are not symbols.
    for child in named_children(root) {
        let node = if child.kind() == "export_statement" {
            child
                .child_by_field_name("declaration")
                .or_else(|| named_children(child).into_iter().next())
                .unwrap_or(child)
        } else {
            child
        };
        classify_ts(node, bytes, defs);
    }
}

fn classify_ts(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            push_def(
                defs,
                field_text(node, "name", bytes),
                node,
                Vec::new(),
                false,
            );
        }
        "class_declaration" | "abstract_class_declaration" => {
            let fields = ts_class_fields(node, bytes);
            let data = !fields.is_empty();
            push_def(defs, field_text(node, "name", bytes), node, fields, data);
        }
        "interface_declaration" => {
            let fields = ts_member_names(node.child_by_field_name("body"), bytes);
            push_def(defs, field_text(node, "name", bytes), node, fields, true);
        }
        "type_alias_declaration" => {
            // A type alias is a data shape only when its value is an object type.
            let value = node.child_by_field_name("value");
            let fields = value
                .filter(|v| v.kind() == "object_type")
                .map(|v| ts_member_names(Some(v), bytes))
                .unwrap_or_default();
            let data = value.map(|v| v.kind() == "object_type").unwrap_or(false);
            push_def(defs, field_text(node, "name", bytes), node, fields, data);
        }
        "enum_declaration" => {
            let fields = ts_member_names(node.child_by_field_name("body"), bytes);
            push_def(defs, field_text(node, "name", bytes), node, fields, true);
        }
        "lexical_declaration" | "variable_declaration" => {
            for d in named_children(node) {
                if d.kind() != "variable_declarator" {
                    continue;
                }
                let value = d.child_by_field_name("value");
                let is_obj = value.map(|v| v.kind() == "object").unwrap_or(false);
                let fields = if is_obj {
                    ts_object_keys(value.unwrap(), bytes)
                } else {
                    Vec::new()
                };
                push_def(defs, field_text(d, "name", bytes), d, fields, is_obj);
            }
        }
        _ => {}
    }
}

fn ts_class_fields(class: Node, bytes: &[u8]) -> Vec<String> {
    let Some(body) = class.child_by_field_name("body") else {
        return Vec::new();
    };
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "public_field_definition" || n.kind() == "field_definition")
        .filter_map(|n| field_text(n, "name", bytes))
        .collect()
}

/// Names of `property_signature` / `enum_assignment` members in an
/// interface_body / object_type / enum_body.
fn ts_member_names(body: Option<Node>, bytes: &[u8]) -> Vec<String> {
    let Some(body) = body else { return Vec::new() };
    named_children(body)
        .into_iter()
        .filter_map(|n| match n.kind() {
            "property_signature" | "enum_assignment" => field_text(n, "name", bytes),
            // Bare enum members appear as a property_identifier child directly.
            "property_identifier" => n.utf8_text(bytes).ok().map(|s| s.to_string()),
            _ => None,
        })
        .collect()
}

fn ts_object_keys(object: Node, bytes: &[u8]) -> Vec<String> {
    named_children(object)
        .into_iter()
        .filter(|n| n.kind() == "pair")
        .filter_map(|n| field_text(n, "key", bytes))
        .filter(|k| is_identifier(k))
        .collect()
}

// --- Python ---

fn collect_python(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    for child in named_children(node) {
        let node = if child.kind() == "decorated_definition" {
            child.child_by_field_name("definition").unwrap_or(child)
        } else {
            child
        };
        match node.kind() {
            "function_definition" => {
                push_def(
                    defs,
                    field_text(node, "name", bytes),
                    node,
                    Vec::new(),
                    false,
                );
            }
            "class_definition" => {
                push_def(
                    defs,
                    field_text(node, "name", bytes),
                    node,
                    Vec::new(),
                    false,
                );
                // Methods are symbols too.
                if let Some(body) = node.child_by_field_name("body") {
                    collect_python(body, bytes, defs);
                }
            }
            _ => {}
        }
    }
}

// --- Generic fallback (go, java, ruby, c, cpp, c#, php) ---

fn collect_generic(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    // Narrow substring set: clearly top-level definition kinds across grammars,
    // deliberately excluding field/property/declarator/variable-level kinds to
    // avoid collecting non-symbols. No field extraction in the fallback.
    const KINDS: &[&str] = &[
        "function",
        "method",
        "class",
        "struct",
        "enum",
        "interface",
        "trait",
    ];
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let kind = n.kind();
        if KINDS.iter().any(|k| kind.contains(k)) {
            push_def(defs, field_text(n, "name", bytes), n, Vec::new(), false);
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

// --- identifier occurrences (reference graph input) ---

fn collect_idents(root: Node, bytes: &[u8], out: &mut Vec<Ident>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "identifier" | "type_identifier" => {
                if let Ok(text) = n.utf8_text(bytes) {
                    out.push(Ident {
                        name: text.to_string(),
                        line: n.start_position().row as u32 + 1,
                    });
                }
            }
            _ => {}
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

// --- qualified path references (cross-container resolution input) ---

/// Walk `use` declarations and flatten each into one or more full segment
/// paths, expanding grouped lists (`use a::{b, c}`) and keeping the real symbol
/// for `x as y` renames. Globs (`use a::*`) are skipped — resolving a glob would
/// require knowing the target's exports, and guessing mints false edges.
fn collect_use_paths(node: Node, bytes: &[u8], out: &mut Vec<PathRef>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "use_declaration" {
            let line = child.start_position().row as u32 + 1;
            if let Some(arg) = child.child_by_field_name("argument") {
                for segments in flatten_use(arg, bytes) {
                    if segments.len() >= 2 {
                        out.push(PathRef { segments, line });
                    }
                }
            }
            continue; // don't descend into the use tree again
        }
        collect_use_paths(child, bytes, out);
    }
}

/// Walk every fully-qualified `scoped_identifier` in expression/type position
/// (NOT inside a `use` decl) — e.g. `scryer_extract::anchors::write_baseline(..)`.
/// This is how cross-crate references most often appear, and unlike a `use` line
/// the reference sits at a real call site, so the resolver can attribute it to
/// the enclosing function.
fn collect_qualified_paths(node: Node, bytes: &[u8], out: &mut Vec<PathRef>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "use_declaration" {
            continue; // handled by collect_use_paths
        }
        // A scoped_identifier nested inside another is just a prefix of it; only
        // flatten the outermost occurrence.
        if child.kind() == "scoped_identifier" && node.kind() != "scoped_identifier" {
            if let Some(segments) = flatten_use(child, bytes).into_iter().next() {
                if segments.len() >= 2 {
                    out.push(PathRef {
                        segments,
                        line: child.start_position().row as u32 + 1,
                    });
                }
            }
        }
        collect_qualified_paths(child, bytes, out);
    }
}

/// Flatten a path subtree (`scoped_identifier`, `scoped_use_list`, `use_list`,
/// `use_as_clause`, leaf identifiers) into the full segment paths it denotes.
fn flatten_use(node: Node, bytes: &[u8]) -> Vec<Vec<String>> {
    match node.kind() {
        "identifier" | "type_identifier" | "crate" | "self" | "super" | "primitive_type" => {
            vec![vec![node.utf8_text(bytes).unwrap_or("").to_string()]]
        }
        "scoped_identifier" => {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .unwrap_or("")
                .to_string();
            let prefixes = match node.child_by_field_name("path") {
                Some(p) => flatten_use(p, bytes),
                None => vec![vec![]],
            };
            prefixes
                .into_iter()
                .map(|mut pre| {
                    pre.push(name.clone());
                    pre
                })
                .collect()
        }
        "scoped_use_list" => {
            let base = match node.child_by_field_name("path") {
                Some(p) => flatten_use(p, bytes).into_iter().next().unwrap_or_default(),
                None => vec![],
            };
            let mut out = Vec::new();
            if let Some(list) = node.child_by_field_name("list") {
                let mut cur = list.walk();
                for item in list.named_children(&mut cur) {
                    for suffix in flatten_use(item, bytes) {
                        let mut full = base.clone();
                        full.extend(suffix);
                        out.push(full);
                    }
                }
            }
            out
        }
        "use_list" => {
            let mut out = Vec::new();
            let mut cur = node.walk();
            for item in node.named_children(&mut cur) {
                out.extend(flatten_use(item, bytes));
            }
            out
        }
        // `x as y`: keep the real symbol path `x` (what the edge targets).
        "use_as_clause" => match node.child_by_field_name("path") {
            Some(p) => flatten_use(p, bytes),
            None => vec![],
        },
        "use_wildcard" => vec![], // glob: skip (see collect_use_paths)
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(parse: &FileParse) -> Vec<&str> {
        parse.defs.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn rust_defs_and_fields() {
        let src = r#"
pub const MAX: u32 = 3;
pub struct Foo { pub a: u32, b: String }
struct Tuple(u32, String);
pub enum Kind { A, B(u32), C { x: u32 } }
pub trait T { fn m(&self); }
pub type Alias = Foo;
pub fn freestanding(x: u32) -> u32 { x }
impl Foo { pub fn method(&self) -> u32 { self.a } }
mod inner { pub fn nested() {} }
"#;
        let p = parse_file(Path::new("f.rs"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"MAX"));
        assert!(n.contains(&"Foo"));
        assert!(n.contains(&"Tuple"));
        assert!(n.contains(&"Kind"));
        assert!(n.contains(&"T"));
        assert!(n.contains(&"Alias"));
        assert!(n.contains(&"freestanding"));
        assert!(n.contains(&"method"), "impl methods are symbols");
        assert!(n.contains(&"nested"), "module items are symbols");

        let foo = p.defs.iter().find(|d| d.name == "Foo").unwrap();
        assert!(foo.is_data_shape);
        assert_eq!(foo.fields, vec!["a", "b"]);

        let kind = p.defs.iter().find(|d| d.name == "Kind").unwrap();
        assert_eq!(kind.fields, vec!["A", "B", "C"]);

        let tuple = p.defs.iter().find(|d| d.name == "Tuple").unwrap();
        assert!(tuple.is_data_shape && tuple.fields.is_empty());
    }

    #[test]
    fn ts_defs_and_fields() {
        let src = r#"
export const useZoom = () => { return 1; };
export const CONFIG = { a: 1, b: "two" };
export function autoLayout(a: number): number { return a; }
function helper() { const local = 5; return local; }
export class Widget { private a: number = 0; render() { return this.a; } }
export interface Lead { name: string; status: number }
export type Alias = { foo: string; bar: number };
export enum Color { Red, Green }
"#;
        let p = parse_file(Path::new("f.ts"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"useZoom"));
        assert!(n.contains(&"CONFIG"));
        assert!(n.contains(&"autoLayout"));
        assert!(n.contains(&"helper"));
        assert!(n.contains(&"Widget"));
        assert!(n.contains(&"Lead"));
        assert!(n.contains(&"Alias"));
        assert!(n.contains(&"Color"));
        assert!(!n.contains(&"local"), "locals are not module-level symbols");

        let cfg = p.defs.iter().find(|d| d.name == "CONFIG").unwrap();
        assert_eq!(cfg.fields, vec!["a", "b"]);
        let lead = p.defs.iter().find(|d| d.name == "Lead").unwrap();
        assert_eq!(lead.fields, vec!["name", "status"]);
        let alias = p.defs.iter().find(|d| d.name == "Alias").unwrap();
        assert_eq!(alias.fields, vec!["foo", "bar"]);
        let color = p.defs.iter().find(|d| d.name == "Color").unwrap();
        assert_eq!(color.fields, vec!["Red", "Green"]);
        let widget = p.defs.iter().find(|d| d.name == "Widget").unwrap();
        assert_eq!(widget.fields, vec!["a"]);
    }

    #[test]
    fn tsx_component() {
        let src = r#"
export function App() { return <div>hi</div>; }
const Card = ({ title }: { title: string }) => <span>{title}</span>;
"#;
        let p = parse_file(Path::new("f.tsx"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"App"));
        assert!(n.contains(&"Card"));
    }

    #[test]
    fn python_defs() {
        let src = r#"
def freestanding(a):
    return a
class Foo:
    field = 0
    def method(self):
        return self.field
"#;
        let p = parse_file(Path::new("f.py"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"freestanding"));
        assert!(n.contains(&"Foo"));
        assert!(n.contains(&"method"));
    }

    /// A path appears in the capture iff its segments match, regardless of line.
    fn has_path(p: &FileParse, segs: &[&str]) -> bool {
        p.paths.iter().any(|pr| pr.segments == segs)
    }

    #[test]
    fn rust_use_paths_captured() {
        let src = r#"
use scryer_core::history::HistoryEvent;
use scryer_core::{drift, build_edges::CachedEdge};
use scryer_extract::anchors::write_baseline as wb;
use std::collections::HashMap;
"#;
        let p = parse_file(Path::new("f.rs"), src).unwrap();
        // plain scoped path
        assert!(has_path(&p, &["scryer_core", "history", "HistoryEvent"]));
        // grouped list expands into one path per leaf, sharing the prefix
        assert!(has_path(&p, &["scryer_core", "drift"]));
        assert!(has_path(&p, &["scryer_core", "build_edges", "CachedEdge"]));
        // `as` rename keeps the real symbol, not the alias
        assert!(has_path(
            &p,
            &["scryer_extract", "anchors", "write_baseline"]
        ));
        assert!(!p
            .paths
            .iter()
            .any(|pr| pr.segments.contains(&"wb".to_string())));
        // std paths are captured too; the resolver (not the parser) filters them
        assert!(has_path(&p, &["std", "collections", "HashMap"]));
    }

    #[test]
    fn rust_qualified_call_site_captured() {
        // The dominant cross-crate form: a fully-qualified path at a call site,
        // with NO `use`. Must be captured with the call-site line.
        let src = r#"
pub fn run() {
    let _ = scryer_extract::anchors::write_baseline(&model);
}
"#;
        let p = parse_file(Path::new("f.rs"), src).unwrap();
        let pr = p
            .paths
            .iter()
            .find(|pr| pr.segments == ["scryer_extract", "anchors", "write_baseline"])
            .expect("qualified call-site path captured");
        assert_eq!(
            pr.line, 3,
            "attributed to the call-site line, not the file top"
        );
    }

    #[test]
    fn rust_glob_use_skipped() {
        let src = "use crate::helpers::*;\nuse scryer_core::prelude::*;\n";
        let p = parse_file(Path::new("f.rs"), src).unwrap();
        assert!(p.paths.is_empty(), "globs carry no unambiguous target");
    }

    #[test]
    fn non_rust_has_no_paths() {
        // Path capture is Rust-only for now; other families stay empty.
        let p = parse_file(Path::new("f.ts"), "import {x} from './y';\n").unwrap();
        assert!(p.paths.is_empty());
    }
}
