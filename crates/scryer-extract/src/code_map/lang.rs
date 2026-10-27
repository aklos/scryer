//! Tree-sitter language registry + definition enumeration.
//!
//! Given a source file, [`parse_file`] returns the top-level *definitions* it
//! declares (functions, types, methods, module-level bindings) with their exact
//! line ranges and — for data shapes (structs, enums, interfaces, type-object
//! literals, classes with fields) — their declared field names. It also returns
//! every identifier occurrence, import and qualified path, which the context
//! resolves into a reference graph, and string-named test blocks (`it("…")`)
//! so a test can be found by its name. A `.vue` file is one component spanning
//! the whole file, its script block parsed in place so line numbers stay
//! file-accurate. Everything here is grammar-derived: no name tables, no regex
//! heuristics on contents.

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
        "clj" | "cljs" | "cljc" | "cljr" => tree_sitter_clojure::LANGUAGE,
        _ => return None,
    };
    Some(f.into())
}

pub fn ext_of(path: &Path) -> Option<&str> {
    path.extension()?.to_str()
}

/// Every extension [`parse_file`] understands: the bundled grammars plus the
/// one composite format handled without a grammar of its own — a Vue
/// single-file component, whose `<script>` block is parsed as TypeScript.
pub fn supports_ext(ext: &str) -> bool {
    ext == VUE_EXT || language_for_ext(ext).is_some()
}

const VUE_EXT: &str = "vue";

/// Import-resolution coverage tier of a source extension: `full` when the
/// link audit sees the language's real declared imports (Rust paths, TS/JS
/// imports + tsconfig aliases, Python module paths, Go module paths, Clojure
/// `ns` requires, C# namespaces and `using` directives),
/// `nameHeuristic` when it only has bare-identifier coincidence within a
/// container — where a real cross-container link can audit as asserted-only.
/// `None` for extensions with no grammar. Health reports this so the audit's
/// verdict is calibrated instead of silently overstated.
pub fn import_resolution_tier(ext: &str) -> Option<&'static str> {
    if !supports_ext(ext) {
        return None;
    }
    Some(match family_for_ext(ext) {
        Family::Rust | Family::TsLike | Family::Python | Family::Go | Family::Clojure | Family::CSharp => "full",
        Family::CLike | Family::Generic => "nameHeuristic",
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Rust,
    TsLike,
    Python,
    Go,
    CSharp,
    CLike,
    Clojure,
    Generic,
}

fn family_for_ext(ext: &str) -> Family {
    match ext {
        "rs" => Family::Rust,
        "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "vue" => Family::TsLike,
        "py" | "pyi" => Family::Python,
        "go" => Family::Go,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Family::CLike,
        "clj" | "cljs" | "cljc" | "cljr" => Family::Clojure,
        "cs" => Family::CSharp,
        _ => Family::Generic,
    }
}

/// One code definition: an addressable symbol.
#[derive(Debug, Clone, Default)]
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
    /// The definition's own doc comment as plain text, one paragraph per
    /// blank-line break: Rust `///` or `/** */` above the item, a JSDoc above
    /// the declaration or its `export`, a Python docstring, a Go comment block.
    pub doc: Option<String>,
    /// What the definition declares inside it, each with its type and doc as
    /// written: a data shape's fields or variants, a React component's props.
    pub members: Vec<Member>,
    /// A React function component: a PascalCase function in a `.tsx`/`.jsx`
    /// file. Its members are its props, and it is no data shape.
    pub is_component: bool,
}

/// One declared member of a definition: a field, a variant, a prop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    /// The type as written, comments dropped and whitespace collapsed; a Rust
    /// variant's payload; `None` when the code declares none.
    pub ty: Option<String>,
    /// The member's own doc comment, as plain text.
    pub doc: Option<String>,
    /// Declared optional (`name?: T` in TypeScript).
    pub optional: bool,
}

/// An identifier occurrence, used to resolve a reference graph between symbols.
#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    /// 1-based line of the occurrence.
    pub line: u32,
}

/// A multi-segment qualified path reference — a Rust `use` path or
/// fully-qualified call/type site (`scryer_extract::anchors::write_baseline`),
/// a Go `pkg.Name` selector/type reference, or a Clojure `alias/sym` call
/// site. The resolver maps the head segment to a container (Rust: the crate
/// manifest map), a package directory (Go: the file's import bindings) or a
/// namespace file (Clojure: the `ns` form's `:as` aliases), and the tail to a
/// symbol, producing the cross-container edges that bare-name resolution
/// cannot. TS/JS and Python emit [`ImportRef`] alone (their specs are module
/// paths, not segment lists); Clojure emits both, since `ns` declares the
/// namespace and usage sites spell the alias. The generic-fallback languages
/// emit neither yet.
#[derive(Debug, Clone)]
pub struct PathRef {
    /// Path segments head-to-leaf, e.g. `["scryer_extract", "anchors", "write_baseline"]`.
    pub segments: Vec<String>,
    /// 1-based line of the reference: the `use` line, or the call/type site.
    pub line: u32,
    /// True when the path came from a `use` declaration — a lexical BINDING
    /// that names the symbol for the rest of the file — rather than a
    /// call/type site. The resolver binds the leaf name on a `use` and
    /// attributes a call site to its enclosing definition.
    pub is_use: bool,
}

/// One module import: a TS/JS `import`/`export … from` / `require(…)` /
/// dynamic `import(…)`, or a Python `import` / `from … import`. The resolver
/// maps the `spec` to a file (relative specs, module paths) or a container
/// (bare package specs, via the declared package-name map), then the `names`
/// to symbols there — the import-flavored counterpart of [`PathRef`]. Only
/// literal specs are captured; a computed spec has no unambiguous target and
/// guessing would mint false edges.
#[derive(Debug, Clone)]
pub struct ImportRef {
    /// Verbatim module specifier. TS/JS: `./zoom`, `../lib/dates`, `@acme/ui`,
    /// `lodash/merge`. Python: a dotted module path, keeping relative-import
    /// dots (`app.util`, `.sibling`, `..pkg.mod`, `.`).
    pub spec: String,
    /// Imported symbols. Empty for whole-module forms — TS namespace
    /// (`* as ns`), side-effect, `export *`; Python `import a.b`, `import *` —
    /// which carry file-level evidence only.
    pub names: Vec<ImportedSym>,
    /// 1-based line of the import statement or call site.
    pub line: u32,
    /// The namespace QUALIFIER this import binds, for languages that bind one
    /// alongside (or instead of) symbols — Clojure's `[app.db :as db]`. Usage
    /// sites spell `db/query`, so the alias is joined against [`PathRef`]
    /// heads rather than resolved here. `None` everywhere else: a TS namespace
    /// import or a Go package qualifier is already carried in `names`.
    pub alias: Option<String>,
    /// A TypeScript import of types alone — `import type { X }`, `export type
    /// { X } from`, or `import { type X, type Y }` with every specifier a type.
    /// It names code without running it: evidence of coupling, never of a
    /// test exercising what it names.
    pub type_only: bool,
}

/// One imported symbol: the name in the SOURCE module (what an edge targets)
/// and the local binding usage sites spell. They differ only for `{ x as y }`
/// renames and `{ key: local }` require-destructures; for a default import or
/// whole-module `require` binding the local name doubles as the (guessed,
/// resolved unique-or-skip) source name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSym {
    pub name: String,
    pub local: String,
}

impl ImportedSym {
    fn same(name: &str) -> Self {
        ImportedSym {
            name: name.to_string(),
            local: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileParse {
    pub defs: Vec<Def>,
    /// String-named call blocks — `it("…")` / `test("…")` / `t.Run("…")` and
    /// kin: any call whose first argument is a literal string, spanning the
    /// whole call (callback body included). This is how attached tests anchor
    /// by their NAME rather than a code identifier; consulted only when a
    /// recorded symbol matches no `defs` entry, never fed to link resolution.
    pub test_blocks: Vec<Def>,
    pub idents: Vec<Ident>,
    /// Qualified path references for cross-container resolution. See [`PathRef`].
    pub paths: Vec<PathRef>,
    /// TS/JS module imports for cross-file/container resolution. See [`ImportRef`].
    pub imports: Vec<ImportRef>,
    /// C# only: the namespaces, `using` directives and top-level types that
    /// decide which names the file can see. See [`CsScope`].
    pub cs: Option<CsScope>,
}

/// What a C# file's names can reach. C# has no per-symbol imports: a file
/// sees every type of its own namespace, of each enclosing namespace, and of
/// each namespace a `using` directive opens (a `global using` opens it for the
/// whole project). The resolver binds a bare name only through these.
#[derive(Debug, Clone, Default)]
pub struct CsScope {
    /// Namespace declarations, dotted (`A.B`, nested blocks joined), each
    /// with the 1-based line span it governs — a file-scoped one runs to EOF.
    pub namespaces: Vec<CsNamespace>,
    pub usings: Vec<CsUsing>,
    /// Types declared directly in a namespace (or the global one), never
    /// nested types or members — the only names a file can reach bare.
    pub types: Vec<CsType>,
    /// Lines holding `using` / `namespace` directives: their identifiers
    /// name namespaces, not types, and never resolve.
    pub directive_lines: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsNamespace {
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsUsing {
    /// The namespace (or, for an alias / `using static`, the type) named.
    pub target: String,
    /// `using Alias = Target;`
    pub alias: Option<String>,
    /// `using static Type;` — opens a type's members, which never resolve bare.
    pub is_static: bool,
    /// `global using …;` — applies to every file of the project.
    pub is_global: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsType {
    pub name: String,
    /// Declaring namespace, dotted; empty for the global namespace.
    pub namespace: String,
    /// 1-based start line — joins the type to its [`Def`].
    pub line: u32,
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
    if ext == VUE_EXT {
        return parse_vue_sfc(path, source, parser);
    }
    let language = language_for_ext(ext)?;
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut defs: Vec<Def> = Vec::new();
    match family_for_ext(ext) {
        Family::Rust => collect_rust(root, bytes, &mut defs),
        Family::TsLike => collect_ts(root, bytes, &mut defs, matches!(ext, "tsx" | "jsx")),
        Family::Python => collect_python(root, bytes, &mut defs),
        Family::Go => collect_go(root, bytes, &mut defs),
        Family::CLike => collect_c(root, bytes, &mut defs),
        Family::Clojure => collect_clojure(root, bytes, &mut defs),
        Family::CSharp | Family::Generic => collect_generic(root, bytes, &mut defs),
    }

    let mut idents: Vec<Ident> = Vec::new();
    match family_for_ext(ext) {
        // Clojure identifiers are `sym_lit`s, not `identifier`s, and a
        // qualified one (`db/query`) contributes its bare leaf here and its
        // alias head to `paths` below.
        Family::Clojure => collect_clj_idents(root, bytes, &mut idents),
        _ => collect_idents(root, bytes, &mut idents),
    }

    // Qualified path references for cross-container resolution: Rust `use` /
    // scoped paths, Go `pkg.Name` selector and type references (resolved
    // through the file's import bindings). TS/JS and Python get the
    // equivalent signal from `imports` below; other families leave both empty.
    let mut paths: Vec<PathRef> = Vec::new();
    match family_for_ext(ext) {
        Family::Rust => {
            collect_use_paths(root, bytes, &mut paths);
            collect_qualified_paths(root, bytes, &mut paths);
        }
        Family::Go => collect_go_paths(root, bytes, &mut paths),
        Family::Clojure => collect_clj_paths(root, bytes, &mut paths),
        _ => {}
    }

    let mut imports: Vec<ImportRef> = Vec::new();
    match family_for_ext(ext) {
        Family::TsLike => collect_ts_imports(root, bytes, &mut imports),
        Family::Python => collect_py_imports(root, bytes, &mut imports),
        Family::Go => collect_go_imports(root, bytes, &mut imports),
        Family::Clojure => collect_clj_imports(root, bytes, &mut imports),
        _ => {}
    }

    let mut test_blocks: Vec<Def> = Vec::new();
    match family_for_ext(ext) {
        Family::Clojure => collect_clj_string_named_forms(root, bytes, &mut test_blocks),
        _ => collect_string_named_calls(root, bytes, &mut test_blocks),
    }

    let cs = (family_for_ext(ext) == Family::CSharp).then(|| collect_cs_scope(root, bytes));

    Some(FileParse {
        defs,
        test_blocks,
        idents,
        paths,
        imports,
        cs,
    })
}

/// A Vue single-file component. The component is the unit — one definition
/// named after the file (`user-card.vue` → `UserCard`) spanning the whole file;
/// the `<template>`/`<style>` blocks never mint symbols of their own. Its
/// `<script>` / `<script setup>` blocks are parsed with the TypeScript grammar
/// for identifier and import references, so `import Foo from "./Foo.vue"`
/// feeds the dependency graph like any other import. The script text is parsed
/// IN PLACE — everything outside the blocks is blanked, not removed — so every
/// reported line number is a line of the `.vue` file itself.
fn parse_vue_sfc(path: &Path, source: &str, parser: &mut Parser) -> Option<FileParse> {
    let name = vue_component_name(path)?;
    let total_lines = source.lines().count().max(1) as u32;
    let def = Def {
        name,
        start_line: 1,
        end_line: total_lines,
        fields: Vec::new(),
        is_data_shape: false,
        ..Default::default()
    };

    let script = vue_script_in_place(source);
    let (mut idents, mut imports) = (Vec::new(), Vec::new());
    if script.trim().is_empty() {
        return Some(FileParse { defs: vec![def], test_blocks: Vec::new(), idents, paths: Vec::new(), imports, cs: None });
    }
    parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).ok()?;
    let tree = parser.parse(&script, None)?;
    let root = tree.root_node();
    let bytes = script.as_bytes();
    collect_idents(root, bytes, &mut idents);
    collect_ts_imports(root, bytes, &mut imports);
    Some(FileParse { defs: vec![def], test_blocks: Vec::new(), idents, paths: Vec::new(), imports, cs: None })
}

/// The SFC's component name: its file stem in PascalCase (`user-card` and
/// `user_card` both → `UserCard`; `UserCard` stays), the name Vue itself
/// resolves the file to and the one an SFC symbol carries in the model.
fn vue_component_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let mut out = String::with_capacity(stem.len());
    let mut upper = true;
    for ch in stem.chars() {
        if ch == '-' || ch == '_' || ch == '.' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The concatenation of every `<script …>` block's content, positioned exactly
/// where it sits in the file: text outside the blocks is replaced by spaces
/// (newlines kept), so byte offsets shift but line numbers do not. Nested
/// `<script>` inside a template string is not a concern — SFC blocks are
/// top-level and a template's own `<script>` would be invalid Vue anyway.
fn vue_script_in_place(source: &str) -> String {
    let mut out: Vec<u8> = source
        .bytes()
        .map(|b| if b == b'\n' { b'\n' } else { b' ' })
        .collect();
    let lower = source.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(open_rel) = lower[cursor..].find("<script") {
        let open = cursor + open_rel;
        // The tag ends at the first `>` after `<script`; a `/>` self-closing
        // tag (`<script src="…" />`) carries no inline content.
        let Some(tag_end_rel) = lower[open..].find('>') else { break };
        let content_start = open + tag_end_rel + 1;
        let self_closing = lower[open..content_start].ends_with("/>");
        let Some(close_rel) = lower[content_start..].find("</script") else { break };
        let content_end = content_start + close_rel;
        if !self_closing {
            out[content_start..content_end].copy_from_slice(&source.as_bytes()[content_start..content_end]);
        }
        cursor = content_end + "</script".len();
    }
    // Lossless by construction: we only ever copy the original bytes back.
    String::from_utf8(out).unwrap_or_default()
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

/// Accepts what the grammars call an identifier: `$`-named JS/TS symbols
/// (`$store`) and unicode identifiers included — the old ASCII-only gate
/// silently dropped their definitions entirely.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn push_def(
    defs: &mut Vec<Def>,
    name: Option<String>,
    node: Node,
    fields: Vec<String>,
    data: bool,
) {
    let members = fields.into_iter().map(|name| Member { name, ..Default::default() }).collect();
    push_documented(defs, name, node, members, data, None);
}

/// Push a definition with its members and doc. A data shape's member names
/// are its `fields`; anything else keeps `fields` empty.
fn push_documented<'d>(
    defs: &'d mut Vec<Def>,
    name: Option<String>,
    node: Node,
    members: Vec<Member>,
    data: bool,
    doc: Option<String>,
) -> Option<&'d mut Def> {
    let name = name.filter(|n| is_identifier(n))?;
    let (start_line, end_line) = line_span(node);
    let fields = if data { members.iter().map(|m| m.name.clone()).collect() } else { Vec::new() };
    defs.push(Def {
        name,
        start_line,
        end_line,
        fields,
        is_data_shape: data,
        doc,
        members,
        is_component: false,
    });
    defs.last_mut()
}

// --- doc comments ---

/// Doc lines as plain text: each paragraph's lines joined with single spaces,
/// a list item (`- `, `* `, `1. `) on a line of its own, paragraphs separated
/// by a blank line, fenced code blocks dropped.
fn doc_text<S: AsRef<str>>(lines: &[S]) -> Option<String> {
    let mut paras: Vec<String> = Vec::new();
    // The paragraph being read, as its lines: one per list item.
    let mut cur: Vec<String> = Vec::new();
    let mut fenced = false;
    let flush = |cur: &mut Vec<String>, paras: &mut Vec<String>| {
        if !cur.is_empty() {
            paras.push(cur.join("\n"));
            cur.clear();
        }
    };
    let is_item = |t: &str| {
        ["- ", "* ", "+ "].iter().any(|m| t.starts_with(m))
            || t.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    for line in lines {
        let t = line.as_ref().trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            flush(&mut cur, &mut paras);
            continue;
        }
        if fenced {
            continue;
        }
        if t.is_empty() {
            flush(&mut cur, &mut paras);
            continue;
        }
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        match cur.last_mut() {
            Some(last) if !is_item(&t) => {
                last.push(' ');
                last.push_str(&t);
            }
            _ => cur.push(t),
        }
    }
    flush(&mut cur, &mut paras);
    (!paras.is_empty()).then(|| paras.join("\n\n"))
}

/// The lines of a `/** … */` (or `/* … */`) block, delimiters and each line's
/// leading `*` dropped.
fn block_comment_lines(text: &str) -> Vec<String> {
    let inner = text.trim_start_matches("/**").trim_start_matches("/*").trim_end_matches("*/");
    inner
        .lines()
        .map(|l| {
            let l = l.trim();
            l.strip_prefix('*').unwrap_or(l).trim().to_string()
        })
        .collect()
}

/// A node's source with its comments cut out and whitespace collapsed: how a
/// type reads as one line of text.
fn code_text(node: Node, bytes: &[u8]) -> String {
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind().contains("comment") {
            cuts.push((n.start_byte(), n.end_byte()));
            continue;
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    cuts.sort();
    let mut out = String::new();
    let mut at = node.start_byte();
    for (a, b) in cuts {
        if a > at {
            out.push_str(&String::from_utf8_lossy(&bytes[at..a]));
        }
        at = at.max(b);
    }
    if node.end_byte() > at {
        out.push_str(&String::from_utf8_lossy(&bytes[at..node.end_byte()]));
    }
    // A trailing separator reads as noise once on one line: `{ a: u32, }`.
    out.split_whitespace().collect::<Vec<_>>().join(" ").replace(", }", " }").replace(", )", ")").replace(", >", " >")
}

/// Whether a node has the anonymous child token `tok` (`?` of an optional
/// property).
fn has_token(node: Node, tok: &str) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| !c.is_named() && c.kind() == tok);
    found
}

/// The `///` lines and `/** */` blocks directly above a Rust item, attributes
/// and plain comments skipped.
fn rust_doc(item: Node, bytes: &[u8]) -> Option<String> {
    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut prev = item.prev_sibling();
    while let Some(p) = prev {
        let t = p.utf8_text(bytes).unwrap_or_default();
        match p.kind() {
            "attribute_item" => {}
            "line_comment" if t.starts_with("///") && !t.starts_with("////") => {
                chunks.push(vec![t.trim_start_matches("///").to_string()]);
            }
            "block_comment" if t.starts_with("/**") && !t.starts_with("/***") && t != "/**/" => {
                chunks.push(block_comment_lines(t));
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        prev = p.prev_sibling();
    }
    chunks.reverse();
    doc_text(&chunks.concat())
}

/// The JSDoc directly above a declaration (or its `export`, or a class or
/// object member): a `/** */` comment ending on the line before, or on the
/// same line. A file-level doc (`@file`, `@module`) documents the file.
fn ts_doc(node: Node, bytes: &[u8]) -> Option<String> {
    let comment = node.prev_named_sibling().filter(|c| c.kind() == "comment")?;
    let t = comment.utf8_text(bytes).ok()?;
    if !t.starts_with("/**") || comment.end_position().row + 1 < node.start_position().row {
        return None;
    }
    if ["@file", "@module", "@overview"].iter().any(|tag| t.contains(tag)) {
        return None;
    }
    let lines: Vec<String> = block_comment_lines(t).into_iter().take_while(|l| !l.starts_with('@')).collect();
    doc_text(&lines)
}

/// The contiguous `//` (or `/* */`) comment block directly above a Go
/// declaration, compiler directives (`//go:…`, `//nolint`) left out.
fn go_doc(node: Node, bytes: &[u8]) -> Option<String> {
    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut cur = node;
    while let Some(p) = cur.prev_sibling() {
        if p.kind() != "comment" || p.end_position().row + 1 != cur.start_position().row {
            break;
        }
        let t = p.utf8_text(bytes).unwrap_or_default();
        let lines = match t.strip_prefix("//") {
            Some(line) if line.starts_with("go:") || line.starts_with("nolint") => Vec::new(),
            Some(line) => vec![line.to_string()],
            None => block_comment_lines(t),
        };
        chunks.push(lines);
        cur = p;
    }
    chunks.reverse();
    doc_text(&chunks.concat())
}

/// A Python definition's docstring: the string opening its body.
fn py_doc(def: Node, bytes: &[u8]) -> Option<String> {
    let first = def.child_by_field_name("body")?.named_child(0)?;
    py_string_doc(first, bytes)
}

/// The text of an expression statement that is a lone string literal.
fn py_string_doc(stmt: Node, bytes: &[u8]) -> Option<String> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let s = stmt.named_child(0).filter(|s| s.kind() == "string")?;
    let content: String = named_children(s)
        .into_iter()
        .filter(|c| c.kind() == "string_content")
        .filter_map(|c| c.utf8_text(bytes).ok())
        .collect();
    let lines: Vec<&str> = content.lines().collect();
    doc_text(&lines)
}

// --- Rust ---

fn collect_rust(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    for child in named_children(node) {
        let doc = || rust_doc(child, bytes);
        match child.kind() {
            "function_item" | "const_item" | "static_item" | "macro_definition" | "type_item"
            | "trait_item" => {
                push_documented(defs, field_text(child, "name", bytes), child, Vec::new(), false, doc());
            }
            "struct_item" => {
                let fields = rust_struct_fields(child, bytes);
                push_documented(defs, field_text(child, "name", bytes), child, fields, true, doc());
            }
            "enum_item" => {
                let variants = rust_enum_variants(child, bytes);
                push_documented(defs, field_text(child, "name", bytes), child, variants, true, doc());
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

/// A named struct's fields with their types and docs. A tuple struct
/// (`ordered_field_declaration_list`) names none.
fn rust_struct_fields(struct_item: Node, bytes: &[u8]) -> Vec<Member> {
    let Some(body) = struct_item.child_by_field_name("body") else {
        return Vec::new();
    };
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "field_declaration")
        .filter_map(|n| {
            Some(Member {
                name: field_text(n, "name", bytes)?,
                ty: n.child_by_field_name("type").map(|t| code_text(t, bytes)),
                doc: rust_doc(n, bytes),
                optional: false,
            })
        })
        .collect()
}

/// An enum's variants with their docs; a tuple or struct payload is the
/// variant's type.
fn rust_enum_variants(enum_item: Node, bytes: &[u8]) -> Vec<Member> {
    let Some(body) = enum_item.child_by_field_name("body") else {
        return Vec::new();
    };
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "enum_variant")
        .filter_map(|n| {
            Some(Member {
                name: field_text(n, "name", bytes)?,
                ty: n.child_by_field_name("body").map(|t| code_text(t, bytes)),
                doc: rust_doc(n, bytes),
                optional: false,
            })
        })
        .collect()
}

// --- TypeScript / JavaScript ---

/// `jsx` marks a `.tsx`/`.jsx` file, where a PascalCase function is a React
/// component.
fn collect_ts(root: Node, bytes: &[u8], defs: &mut Vec<Def>, jsx: bool) {
    // Module level only: direct children of the program, unwrapping exports.
    // Descending further would pick up locals (every const/let is a
    // variable_declarator) which are not symbols.
    // The file's own named types, for a component whose props type is one.
    let mut shapes: std::collections::HashMap<String, Node> = std::collections::HashMap::new();
    if jsx {
        for child in named_children(root) {
            let node = ts_unwrap_export(child);
            let body = match node.kind() {
                "interface_declaration" => node.child_by_field_name("body"),
                "type_alias_declaration" => node.child_by_field_name("value"),
                _ => None,
            };
            if let (Some(name), Some(body)) = (field_text(node, "name", bytes), body) {
                shapes.insert(name, body);
            }
        }
    }
    for child in named_children(root) {
        let ctx = TsCtx { bytes, jsx, shapes: &shapes, outer: child };
        classify_ts(ts_unwrap_export(child), &ctx, defs);
    }
}

/// The declaration an `export` statement carries, or the statement itself.
fn ts_unwrap_export(child: Node) -> Node {
    if child.kind() == "export_statement" {
        child
            .child_by_field_name("declaration")
            .or_else(|| named_children(child).into_iter().next())
            .unwrap_or(child)
    } else {
        child
    }
}

struct TsCtx<'a, 't> {
    bytes: &'a [u8],
    jsx: bool,
    shapes: &'a std::collections::HashMap<String, Node<'t>>,
    /// The top-level statement (the `export` itself, when exported) whose
    /// JSDoc documents the declaration.
    outer: Node<'t>,
}

fn classify_ts(node: Node, ctx: &TsCtx, defs: &mut Vec<Def>) {
    let bytes = ctx.bytes;
    let doc = || ts_doc(ctx.outer, bytes);
    let name = field_text(node, "name", bytes);
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            if ctx.jsx && name.as_deref().is_some_and(is_pascal_case) {
                let props = component_props(node, None, ctx);
                if let Some(def) = push_documented(defs, name, node, props, false, doc()) {
                    def.is_component = true;
                }
            } else {
                push_documented(defs, name, node, Vec::new(), false, doc());
            }
        }
        "class_declaration" | "abstract_class_declaration" => {
            let fields = ts_class_fields(node, bytes);
            let data = !fields.is_empty();
            push_documented(defs, name, node, fields, data, doc());
        }
        "interface_declaration" => {
            let fields = ts_members(node.child_by_field_name("body"), bytes);
            push_documented(defs, name, node, fields, true, doc());
        }
        "type_alias_declaration" => {
            // A type alias is a data shape only when its value is an object type.
            let value = node.child_by_field_name("value").filter(|v| v.kind() == "object_type");
            let fields = value.map(|v| ts_members(Some(v), bytes)).unwrap_or_default();
            push_documented(defs, name, node, fields, value.is_some(), doc());
        }
        "enum_declaration" => {
            let fields = ts_members(node.child_by_field_name("body"), bytes);
            push_documented(defs, name, node, fields, true, doc());
        }
        "lexical_declaration" | "variable_declaration" => {
            for d in named_children(node) {
                if d.kind() != "variable_declarator" {
                    continue;
                }
                let name = field_text(d, "name", bytes);
                let value = d.child_by_field_name("value");
                if ctx.jsx && name.as_deref().is_some_and(is_pascal_case) {
                    if let Some(func) = value.and_then(component_function) {
                        let props = component_props(func, Some(d), ctx);
                        if let Some(def) = push_documented(defs, name, d, props, false, doc()) {
                            def.is_component = true;
                        }
                        continue;
                    }
                }
                let object = value.filter(|v| v.kind() == "object");
                let fields = object.map(|v| ts_object_keys(v, bytes)).unwrap_or_default();
                push_documented(defs, name, d, fields, object.is_some(), doc());
            }
        }
        _ => {}
    }
}

/// `UserCard`, not `userCard` or `API_URL`.
fn is_pascal_case(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase()) && name.chars().any(|c| c.is_lowercase())
}

/// The function a component binding holds: an arrow or function expression,
/// or one wrapped in a call (`memo(…)`, `forwardRef(…)`).
fn component_function(value: Node) -> Option<Node> {
    match value.kind() {
        "arrow_function" | "function_expression" | "function" => Some(value),
        "call_expression" => value
            .child_by_field_name("arguments")
            .and_then(|a| a.named_child(0))
            .and_then(component_function),
        _ => None,
    }
}

/// A component's props: its first parameter's type — an inline object type,
/// or a named interface or type alias of the same file — else the names it
/// destructures; failing both, the type argument of the binding's own
/// annotation (`const X: FC<Props> = …`).
fn component_props(func: Node, declarator: Option<Node>, ctx: &TsCtx) -> Vec<Member> {
    let first = func
        .child_by_field_name("parameters")
        .and_then(|p| named_children(p).into_iter().find(|c| c.kind() != "comment"))
        .or_else(|| func.child_by_field_name("parameter"));
    let mut props = Vec::new();
    if let Some(param) = first {
        if let Some(ty) = param.child_by_field_name("type").and_then(|a| a.named_child(0)) {
            props = props_of_type(ty, ctx, 0);
        }
        if props.is_empty() {
            let mut pattern = param.child_by_field_name("pattern").unwrap_or(param);
            if pattern.kind() == "assignment_pattern" {
                pattern = pattern.child_by_field_name("left").unwrap_or(pattern);
            }
            if pattern.kind() == "object_pattern" {
                props = destructured_names(pattern, ctx.bytes);
            }
        }
    }
    if props.is_empty() {
        let annotated = declarator
            .and_then(|d| d.child_by_field_name("type"))
            .and_then(|a| a.named_child(0))
            .filter(|t| t.kind() == "generic_type");
        if let Some(arg) = annotated.and_then(type_argument) {
            props = props_of_type(arg, ctx, 0);
        }
    }
    props
}

/// The first type argument of a generic type (`FC<Props>` → `Props`).
fn type_argument(generic: Node) -> Option<Node> {
    let args = named_children(generic).into_iter().find(|c| c.kind() == "type_arguments")?;
    args.named_child(0)
}

/// The members a props type declares, a named type of the same file resolved
/// (one alias deep per step, at most a few steps).
fn props_of_type(ty: Node, ctx: &TsCtx, depth: u8) -> Vec<Member> {
    if depth > 3 {
        return Vec::new();
    }
    let bytes = ctx.bytes;
    match ty.kind() {
        "object_type" | "interface_body" => ts_members(Some(ty), bytes),
        "type_identifier" => ty
            .utf8_text(bytes)
            .ok()
            .and_then(|name| ctx.shapes.get(name))
            .map(|body| props_of_type(*body, ctx, depth + 1))
            .unwrap_or_default(),
        "generic_type" => {
            let own = ty
                .child_by_field_name("name")
                .filter(|n| n.kind() == "type_identifier")
                .map(|n| props_of_type(n, ctx, depth + 1))
                .unwrap_or_default();
            if own.is_empty() {
                // `PropsWithChildren<Props>`: the props are the argument's.
                type_argument(ty).map(|a| props_of_type(a, ctx, depth + 1)).unwrap_or_default()
            } else {
                own
            }
        }
        "intersection_type" => {
            named_children(ty).into_iter().flat_map(|t| props_of_type(t, ctx, depth + 1)).collect()
        }
        "parenthesized_type" => {
            ty.named_child(0).map(|t| props_of_type(t, ctx, depth + 1)).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// The names an object pattern destructures: `{ title, onClose: close, size = 1 }`
/// gives `title`, `onClose`, `size`.
fn destructured_names(pattern: Node, bytes: &[u8]) -> Vec<Member> {
    named_children(pattern)
        .into_iter()
        .filter_map(|n| {
            let name_node = match n.kind() {
                "shorthand_property_identifier_pattern" | "shorthand_property_identifier" => Some(n),
                "pair_pattern" => n.child_by_field_name("key"),
                "object_assignment_pattern" => n.child_by_field_name("left"),
                _ => None,
            }?;
            let name = name_node.utf8_text(bytes).ok()?.to_string();
            is_identifier(&name).then(|| Member { name, ..Default::default() })
        })
        .collect()
}

fn ts_class_fields(class: Node, bytes: &[u8]) -> Vec<Member> {
    let Some(body) = class.child_by_field_name("body") else {
        return Vec::new();
    };
    named_children(body)
        .into_iter()
        .filter(|n| n.kind() == "public_field_definition" || n.kind() == "field_definition")
        .filter_map(|n| {
            let name = field_text(n, "name", bytes).or_else(|| field_text(n, "property", bytes))?;
            Some(Member {
                name,
                ty: ts_annotation(n, bytes),
                doc: ts_doc(n, bytes),
                optional: has_token(n, "?"),
            })
        })
        .collect()
}

/// The type a `: T` annotation on `node` gives, as text.
fn ts_annotation(node: Node, bytes: &[u8]) -> Option<String> {
    let annotation = node.child_by_field_name("type")?;
    let ty = if annotation.kind() == "type_annotation" { annotation.named_child(0)? } else { annotation };
    Some(code_text(ty, bytes))
}

/// The `property_signature` / `enum_assignment` members of an interface_body
/// / object_type / enum_body, with their types and JSDoc.
fn ts_members(body: Option<Node>, bytes: &[u8]) -> Vec<Member> {
    let Some(body) = body else { return Vec::new() };
    named_children(body)
        .into_iter()
        .filter_map(|n| {
            let (name, ty, optional) = match n.kind() {
                "property_signature" => (field_text(n, "name", bytes)?, ts_annotation(n, bytes), has_token(n, "?")),
                "enum_assignment" => (field_text(n, "name", bytes)?, None, false),
                // Bare enum members appear as a property_identifier child directly.
                "property_identifier" => (n.utf8_text(bytes).ok()?.to_string(), None, false),
                _ => return None,
            };
            Some(Member { name, ty, doc: ts_doc(n, bytes), optional })
        })
        .collect()
}

fn ts_object_keys(object: Node, bytes: &[u8]) -> Vec<Member> {
    named_children(object)
        .into_iter()
        .filter(|n| n.kind() == "pair")
        .filter_map(|n| field_text(n, "key", bytes).map(|k| (n, k)))
        .filter(|(_, k)| is_identifier(k))
        .map(|(n, name)| Member { name, doc: ts_doc(n, bytes), ..Default::default() })
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
                push_documented(defs, field_text(node, "name", bytes), node, Vec::new(), false, py_doc(node, bytes));
            }
            "class_definition" => {
                let fields = if py_is_record(child, node, bytes) { py_class_fields(node, bytes) } else { Vec::new() };
                let data = !fields.is_empty();
                push_documented(defs, field_text(node, "name", bytes), node, fields, data, py_doc(node, bytes));
                // Methods are symbols too.
                if let Some(body) = node.child_by_field_name("body") {
                    collect_python(body, bytes, defs);
                }
            }
            _ => {}
        }
    }
}

/// A class that is a record of typed fields: a `@dataclass`, or a pydantic
/// `BaseModel`, `TypedDict` or `NamedTuple` subclass.
fn py_is_record(outer: Node, class: Node, bytes: &[u8]) -> bool {
    let decorated = outer.kind() == "decorated_definition"
        && named_children(outer)
            .into_iter()
            .filter(|d| d.kind() == "decorator")
            .any(|d| d.utf8_text(bytes).is_ok_and(|t| t.contains("dataclass")));
    let based = class.child_by_field_name("superclasses").is_some_and(|s| {
        named_children(s).into_iter().any(|b| {
            let t = b.utf8_text(bytes).unwrap_or_default();
            let leaf = t.rsplit('.').next().unwrap_or(t);
            matches!(leaf, "BaseModel" | "TypedDict" | "NamedTuple")
        })
    });
    decorated || based
}

/// A record class's annotated fields (`name: str = ""`), each documented by
/// the string statement right after it, when there is one.
fn py_class_fields(class: Node, bytes: &[u8]) -> Vec<Member> {
    let Some(body) = class.child_by_field_name("body") else { return Vec::new() };
    let stmts = named_children(body);
    stmts
        .iter()
        .enumerate()
        .filter_map(|(i, stmt)| {
            let assign = stmt.named_child(0).filter(|a| stmt.kind() == "expression_statement" && a.kind() == "assignment")?;
            let ty = assign.child_by_field_name("type")?;
            let left = assign.child_by_field_name("left").filter(|l| l.kind() == "identifier")?;
            Some(Member {
                name: left.utf8_text(bytes).ok()?.to_string(),
                ty: Some(code_text(ty, bytes)),
                doc: stmts.get(i + 1).and_then(|next| py_string_doc(*next, bytes)),
                optional: false,
            })
        })
        .collect()
}

// --- Go ---

/// Top-level Go definitions. The generic fallback used to miss every struct
/// and interface (`type_spec` has the name, and nothing in the fallback's
/// kind set matches it) while minting bogus defs from interface method specs
/// (`method_elem` matched "method") — Go earns its own collector.
fn collect_go(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    for child in named_children(node) {
        match child.kind() {
            "function_declaration" | "method_declaration" => {
                push_documented(defs, field_text(child, "name", bytes), child, Vec::new(), false, go_doc(child, bytes));
            }
            "type_declaration" => {
                for spec in named_children(child) {
                    if !matches!(spec.kind(), "type_spec" | "type_alias") {
                        continue;
                    }
                    // A grouped `type ( … )` documents each spec inside it; a
                    // single one is documented above the `type` keyword.
                    let doc = go_doc(spec, bytes).or_else(|| go_doc(child, bytes));
                    // Span the SPEC, not the whole declaration: a grouped
                    // `type ( A …; B … )` block holds several defs.
                    let type_node = spec.child_by_field_name("type");
                    match type_node.filter(|t| t.kind() == "struct_type") {
                        Some(st) => {
                            let fields = go_struct_fields(st, bytes);
                            push_documented(defs, field_text(spec, "name", bytes), spec, fields, true, doc);
                        }
                        // An interface is a method set — behavioral, like a
                        // Rust trait, not a data shape.
                        None => {
                            push_documented(defs, field_text(spec, "name", bytes), spec, Vec::new(), false, doc);
                        }
                    }
                }
            }
            "const_declaration" | "var_declaration" => {
                for spec in named_children(child) {
                    if matches!(spec.kind(), "const_spec" | "var_spec") {
                        let doc = go_doc(spec, bytes).or_else(|| go_doc(child, bytes));
                        push_documented(defs, field_text(spec, "name", bytes), spec, Vec::new(), false, doc);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Declared fields of a `struct_type`, with their type and doc. A
/// `field_declaration` may bind several names to one type (`X, Y int`);
/// embedded fields declare no `field_identifier` and contribute none.
fn go_struct_fields(struct_type: Node, bytes: &[u8]) -> Vec<Member> {
    let Some(list) = named_children(struct_type)
        .into_iter()
        .find(|n| n.kind() == "field_declaration_list")
    else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    for decl in named_children(list) {
        if decl.kind() != "field_declaration" {
            continue;
        }
        let ty = decl.child_by_field_name("type").map(|t| code_text(t, bytes));
        let doc = go_doc(decl, bytes);
        for c in named_children(decl) {
            if c.kind() == "field_identifier" {
                if let Ok(text) = c.utf8_text(bytes) {
                    fields.push(Member { name: text.to_string(), ty: ty.clone(), doc: doc.clone(), optional: false });
                }
            }
        }
    }
    fields
}

/// Go import declarations. Each spec binds a package QUALIFIER, not symbols:
/// the binding is the declared alias or the path's last segment (a heuristic —
/// a package may declare a different name than its directory; under-report
/// when it does). Dot-imports merge the package into file scope and blank
/// imports bind nothing — both yield a spec with no names.
fn collect_go_imports(node: Node, bytes: &[u8], out: &mut Vec<ImportRef>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "import_spec" {
            let Some(spec) = n
                .child_by_field_name("path")
                .and_then(|p| p.utf8_text(bytes).ok())
                .map(|s| s.trim_matches('"').to_string())
            else {
                continue;
            };
            let binding = match n.child_by_field_name("name") {
                Some(name) if name.kind() == "package_identifier" => {
                    name.utf8_text(bytes).ok().map(|s| s.to_string())
                }
                Some(_) => None, // dot or blank import
                None => spec.rsplit('/').next().map(|s| s.to_string()),
            };
            out.push(ImportRef {
                names: binding.map(|b| vec![ImportedSym::same(&b)]).unwrap_or_default(),
                spec,
                line: n.start_position().row as u32 + 1,
                alias: None,
                type_only: false,
            });
            continue;
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// Qualified references — `pkg.Name` selectors whose operand is a plain
/// identifier, and `pkg.Type` qualified type nodes. Segments are
/// `[qualifier, name]`; the resolver joins the qualifier against the file's
/// import bindings, so a selector on a local variable simply fails the join.
fn collect_go_paths(node: Node, bytes: &[u8], out: &mut Vec<PathRef>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let pair = match n.kind() {
            "selector_expression" => {
                let operand = n.child_by_field_name("operand");
                let field = n.child_by_field_name("field");
                match (operand, field) {
                    (Some(o), Some(f)) if o.kind() == "identifier" => Some((o, f)),
                    _ => None,
                }
            }
            "qualified_type" => {
                let pkg = n.child_by_field_name("package");
                let name = n.child_by_field_name("name");
                match (pkg, name) {
                    (Some(p), Some(t)) => Some((p, t)),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((qualifier, name)) = pair {
            if let (Ok(q), Ok(nm)) = (qualifier.utf8_text(bytes), name.utf8_text(bytes)) {
                out.push(PathRef {
                    segments: vec![q.to_string(), nm.to_string()],
                    line: n.start_position().row as u32 + 1,
                    is_use: false,
                });
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

// --- C / C++ ---

/// C/C++ definitions. The generic fallback extracted NO functions here —
/// tree-sitter's C grammars put the name under `declarator`, not a `name`
/// field — while `struct foo x;` use sites minted phantom one-line defs
/// (`struct_specifier` matched "struct" and has a `name`). Only specifiers
/// WITH a body are definitions; function bodies are not descended into, so
/// locals never become symbols.
fn collect_c(root: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "function_definition" => {
                if let Some(declarator) = n.child_by_field_name("declarator") {
                    push_def(defs, c_declarator_name(declarator, bytes), n, Vec::new(), false);
                }
                continue; // never descend into bodies: locals are not symbols
            }
            // `typedef struct {…} Foo;` — the new name lives in the declarator;
            // a named underlying struct is picked up by the descent.
            "type_definition" => {
                if let Some(declarator) = n.child_by_field_name("declarator") {
                    push_def(defs, c_declarator_name(declarator, bytes), n, Vec::new(), false);
                }
            }
            "struct_specifier" | "union_specifier" | "class_specifier" => {
                if let (Some(name), Some(body)) =
                    (field_text(n, "name", bytes), n.child_by_field_name("body"))
                {
                    let fields = c_struct_fields(body, bytes);
                    push_def(defs, Some(name), n, fields, true);
                }
            }
            "enum_specifier" => {
                if let (Some(name), Some(body)) =
                    (field_text(n, "name", bytes), n.child_by_field_name("body"))
                {
                    let fields = named_children(body)
                        .into_iter()
                        .filter(|e| e.kind() == "enumerator")
                        .filter_map(|e| field_text(e, "name", bytes))
                        .collect();
                    push_def(defs, Some(name), n, fields, true);
                }
            }
            _ => {}
        }
        // Descend everywhere else: namespaces, class bodies (inline methods),
        // extern "C" blocks, preprocessor conditionals.
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// The name inside a (possibly nested) C/C++ declarator: `*foo(...)`,
/// `Foo::bar(...)`, `(*fp)(...)` — unwrap declarator wrappers until an
/// identifier-like node (or the `name` of a qualified one) appears.
fn c_declarator_name(node: Node, bytes: &[u8]) -> Option<String> {
    let mut cur = node;
    loop {
        match cur.kind() {
            "identifier" | "field_identifier" | "type_identifier" => {
                return cur.utf8_text(bytes).ok().map(|s| s.to_string());
            }
            // `Foo::bar` — the def's own name is the last segment.
            "qualified_identifier" => cur = cur.child_by_field_name("name")?,
            _ => cur = cur.child_by_field_name("declarator")?,
        }
    }
}

/// Declared field names of a struct/union/class body, skipping method
/// definitions and anonymous members. One declaration can bind several names
/// (`int x, y;`) — the `declarator` field repeats.
fn c_struct_fields(body: Node, bytes: &[u8]) -> Vec<String> {
    let mut fields = Vec::new();
    for decl in named_children(body) {
        if decl.kind() != "field_declaration" {
            continue;
        }
        let mut cur = decl.walk();
        for declarator in decl.children_by_field_name("declarator", &mut cur) {
            if let Some(name) = c_declarator_name(declarator, bytes) {
                fields.push(name);
            }
        }
    }
    fields
}

// --- Generic fallback (java, ruby, c#, php) ---

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

// --- C# name scope (namespace + using resolution input) ---

const CS_TYPE_KINDS: &[&str] = &[
    "class_declaration",
    "struct_declaration",
    "record_declaration",
    "record_struct_declaration",
    "interface_declaration",
    "enum_declaration",
    "delegate_declaration",
];

/// Walk a C# compilation unit for its namespaces, `using` directives and the
/// types declared directly in a namespace. Nested types and members are left
/// out on purpose: C# reaches them only through their declaring type, which
/// is the edge that matters.
pub fn collect_cs_scope(root: Node, bytes: &[u8]) -> CsScope {
    let mut scope = CsScope::default();
    let eof = root.end_position().row as u32 + 1;
    cs_walk(root, bytes, "", eof, &mut scope);
    scope
}

fn cs_walk(node: Node, bytes: &[u8], ns: &str, eof: u32, scope: &mut CsScope) {
    // A file-scoped `namespace X;` governs every sibling after it.
    let mut current = ns.to_string();
    for child in named_children(node) {
        let (start, end) = line_span(child);
        match child.kind() {
            "using_directive" => {
                scope.directive_lines.push(start);
                if let Some(u) = cs_using(child, bytes) {
                    scope.usings.push(u);
                }
            }
            "file_scoped_namespace_declaration" => {
                scope.directive_lines.push(start);
                if let Some(name) = field_text(child, "name", bytes) {
                    current = cs_join(ns, &name);
                    scope.namespaces.push(CsNamespace { name: current.clone(), start_line: start, end_line: eof });
                }
            }
            "namespace_declaration" => {
                scope.directive_lines.push(start);
                let Some(name) = field_text(child, "name", bytes) else { continue };
                let full = cs_join(&current, &name);
                scope.namespaces.push(CsNamespace { name: full.clone(), start_line: start, end_line: end });
                if let Some(body) = child.child_by_field_name("body") {
                    cs_walk(body, bytes, &full, eof, scope);
                }
            }
            k if CS_TYPE_KINDS.contains(&k) => {
                if let Some(name) = field_text(child, "name", bytes) {
                    scope.types.push(CsType { name, namespace: current.clone(), line: start });
                }
            }
            _ => {}
        }
    }
}

fn cs_join(outer: &str, inner: &str) -> String {
    let inner: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
    if outer.is_empty() { inner } else { format!("{outer}.{inner}") }
}

fn cs_using(node: Node, bytes: &[u8]) -> Option<CsUsing> {
    let mut is_global = false;
    let mut is_static = false;
    let mut target = None;
    let alias = field_text(node, "name", bytes);
    let alias_node = node.child_by_field_name("name");
    let mut cur = node.walk();
    for c in node.children(&mut cur) {
        match c.kind() {
            "global" => is_global = true,
            "static" => is_static = true,
            _ if c.is_named() && Some(c) != alias_node => {
                target = c.utf8_text(bytes).ok().map(|t| t.chars().filter(|ch| !ch.is_whitespace()).collect());
            }
            _ => {}
        }
    }
    Some(CsUsing { target: target?, alias, is_static, is_global })
}

// --- identifier occurrences (reference graph input) ---

// --- Clojure ---
//
// tree-sitter-clojure is primitives-only BY DESIGN: `defn` lives in
// `clojure.core` as a macro, not in the language's syntax, so the grammar
// exposes `list_lit` / `vec_lit` / `sym_lit` and stops there. There is no
// `function_definition` node and no `name` field to read, which is why both
// `collect_generic` (matches node kinds containing "function"/"class") and
// `collect_idents` (matches kind `identifier`) find exactly nothing here.
//
// Definitions are therefore recognized STRUCTURALLY: a list whose head symbol
// is a `def…` form, taking the following symbol as the name. That shape is not
// a workaround — it is the only rule that can work, and it earns something a
// fixed node-kind list could never have: user-defined defining macros
// (`defroutes`, `defstate`, `defcomponent`, `defsc`) resolve for free.

/// Head symbols that read as `def…` but define nothing.
const CLJ_NON_DEF_HEADS: &[&str] = &["default"];

/// `ns` reference forms whose bodies name other Clojure namespaces. `:use` is
/// the pre-1.4 spelling and still appears; `:require-macros` is ClojureScript.
const CLJ_REQUIRE_KEYS: &[&str] = &[":require", ":require-macros", ":use"];

/// Head symbols of top-level namespace-loading calls, outside any `ns` form.
const CLJ_REQUIRE_HEADS: &[&str] = &["require", "use", "require-macros"];

/// The bare name of a `sym_lit`: its `sym_name` child, with any namespace
/// qualifier dropped (`query` for `db/query`, `def` for `s/def`). The
/// qualifier is recovered separately by [`collect_clj_paths`].
fn clj_sym_name(node: Node, bytes: &[u8]) -> Option<String> {
    clj_child_text(node, "sym_name", bytes)
}

/// The namespace qualifier of a `sym_lit` — `db` in `db/query`, `None` for a
/// bare symbol.
fn clj_sym_ns(node: Node, bytes: &[u8]) -> Option<String> {
    clj_child_text(node, "sym_ns", bytes)
}

fn clj_child_text(node: Node, kind: &str, bytes: &[u8]) -> Option<String> {
    named_children(node)
        .into_iter()
        .find(|c| c.kind() == kind)?
        .utf8_text(bytes)
        .ok()
        .map(|s| s.to_string())
}

/// A form's meaningful children. Metadata and comments attach to the form
/// rather than acting as arguments of it, so `(defn ^:private f [x])` still
/// reads as head / name / params once they're dropped.
fn clj_payload<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    named_children(node)
        .into_iter()
        .filter(|c| !matches!(c.kind(), "comment" | "meta_lit" | "old_meta_lit"))
        .collect()
}

/// The head of a form when it is a plain symbol, namespace qualifier dropped.
fn clj_head(node: Node, bytes: &[u8]) -> Option<String> {
    let head = *clj_payload(node).first()?;
    if head.kind() != "sym_lit" {
        return None;
    }
    clj_sym_name(head, bytes)
}

/// The head of a form when it is a keyword — `:require` in `(:require …)`.
fn clj_head_kwd(node: Node, bytes: &[u8]) -> Option<String> {
    let head = *clj_payload(node).first()?;
    if head.kind() != "kwd_lit" {
        return None;
    }
    head.utf8_text(bytes).ok().map(|s| s.to_string())
}

/// Subtrees holding DATA rather than evaluated code: quoted forms and
/// reader-discarded forms. A `defn` inside one defines nothing — this is
/// precisely the `defn`-as-macro-argument ambiguity that keeps higher-level
/// constructs out of the grammar, and skipping these subtrees is how we avoid
/// inheriting it.
fn clj_is_unevaluated(kind: &str) -> bool {
    matches!(
        kind,
        "quoting_lit" | "syn_quoting_lit" | "var_quoting_lit" | "dis_expr"
    )
}

/// A plausible Clojure name. The grammar has already established that the
/// token IS a symbol or keyword, so this only rejects degenerate cases — the
/// `is_identifier` gate the other families use would discard most real
/// Clojure names (`fetch-user`, `put!`, `empty?`, `*warn-on-reflection*`).
fn is_clj_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && !s.contains(|c: char| c.is_whitespace())
}

/// Is this head symbol a defining form?
///
/// `declare` is deliberately excluded: a forward declaration has no body, so
/// minting it would put a same-named peer at a span that holds no definition —
/// inflating `peers` and muddying the anchor checker's "my def was deleted
/// while a sibling survived" test.
fn clj_is_def_head(head: &str) -> bool {
    head.starts_with("def") && !CLJ_NON_DEF_HEADS.contains(&head)
}

/// The text of a `str_lit`, surrounding quotes trimmed.
fn clj_str_text(node: Node, bytes: &[u8]) -> Option<String> {
    let raw = node.utf8_text(bytes).ok()?;
    let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"'))?;
    (!inner.is_empty()).then(|| inner.to_string())
}

fn collect_clojure(root: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if clj_is_unevaluated(n.kind()) {
            continue;
        }
        if n.kind() == "list_lit" {
            if let Some(head) = clj_head(n, bytes) {
                // `(comment …)` is Clojure's rich-comment idiom: a scratch
                // buffer that happens to be readable code. Its defs are not
                // definitions, and minting them would anchor claims to
                // throwaway experiments.
                if head == "comment" {
                    continue;
                }
                if clj_is_def_head(&head) {
                    push_clj_def(n, bytes, &head, defs);
                    continue; // a def's body holds no further definitions
                }
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
    // The walk is a stack, so emit in document order for a stable extraction.
    defs.sort_by(|a, b| (a.start_line, &a.name).cmp(&(b.start_line, &b.name)));
}

fn push_clj_def(list: Node, bytes: &[u8], head: &str, defs: &mut Vec<Def>) {
    let kids = clj_payload(list);
    let Some(&name_node) = kids.get(1) else {
        return;
    };
    let name = match name_node.kind() {
        "sym_lit" => clj_sym_name(name_node, bytes),
        // `(s/def ::email string?)` — a spec is named by a keyword, and that
        // keyword is what a sourceMap anchor would spell for it.
        "kwd_lit" => name_node.utf8_text(bytes).ok().map(|s| s.to_string()),
        _ => None,
    };
    let Some(name) = name.filter(|n| is_clj_name(n)) else {
        return;
    };

    // Data shapes: a record/type declares positional fields in a vector, a
    // protocol/interface declares method signatures as inner lists.
    let is_data_shape = matches!(
        head,
        "defrecord" | "deftype" | "defstruct" | "defprotocol" | "definterface"
    );
    let mut fields: Vec<String> = Vec::new();
    if is_data_shape {
        for k in &kids[2..] {
            match k.kind() {
                "vec_lit" => {
                    fields.extend(
                        clj_payload(*k)
                            .into_iter()
                            .filter(|f| f.kind() == "sym_lit")
                            .filter_map(|f| clj_sym_name(f, bytes)),
                    );
                    // The FIRST vector is the field list; later ones are the
                    // parameter vectors of inline protocol implementations.
                    break;
                }
                "list_lit" => {
                    if let Some(method) = clj_head(*k, bytes) {
                        fields.push(method);
                    }
                }
                _ => {} // docstring, options map
            }
        }
    }
    let (start_line, end_line) = line_span(list);
    defs.push(Def {
        name,
        start_line,
        end_line,
        fields,
        is_data_shape,
        ..Default::default()
    });
}

/// Every UNQUALIFIED symbol occurrence — the input to the bare-name scopes,
/// which is where a same-namespace reference resolves.
///
/// A qualified symbol (`db/query`, `lib/query`) is deliberately excluded: it
/// is exact evidence and [`collect_clj_paths`] resolves it through the `ns`
/// form's alias bindings. Contributing its leaf here as well would let
/// `lib/query` — a call into an external library — resolve by bare-name
/// coincidence to a same-named local def, minting an edge that does not exist.
fn collect_clj_idents(root: Node, bytes: &[u8], out: &mut Vec<Ident>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if clj_is_unevaluated(n.kind()) {
            continue;
        }
        if n.kind() == "sym_lit" && clj_sym_ns(n, bytes).is_none() {
            if let Some(name) = clj_sym_name(n, bytes).filter(|s| is_clj_name(s)) {
                out.push(Ident {
                    name,
                    line: n.start_position().row as u32 + 1,
                });
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// Namespace-qualified references (`db/query`, `str/join`) — the Clojure
/// counterpart of a Go `pkg.Name` selector. The head segment is an alias the
/// file's own `ns` form bound, so the resolver can follow it to one namespace
/// file and resolve the leaf there.
fn collect_clj_paths(root: Node, bytes: &[u8], out: &mut Vec<PathRef>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if clj_is_unevaluated(n.kind()) {
            continue;
        }
        if n.kind() == "sym_lit" {
            if let (Some(ns), Some(name)) = (clj_sym_ns(n, bytes), clj_sym_name(n, bytes)) {
                if is_clj_name(&ns) && is_clj_name(&name) {
                    out.push(PathRef {
                        segments: vec![ns, name],
                        line: n.start_position().row as u32 + 1,
                        // A qualified `ns/name` reference is a call site, not a binding.
                        is_use: false,
                    });
                }
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// The `ns` form's namespace references, plus top-level `(require …)`.
///
/// `(ns app.core (:require [app.db :as db] [clojure.string :refer [join]]))`
/// yields one [`ImportRef`] per required namespace: `spec` is the namespace (a
/// dotted path the resolver maps to a file), `names` the `:refer`red symbols
/// (which bind bare locals), `alias` the `:as` qualifier (used as `db/query`,
/// joined against [`PathRef`] heads).
///
/// `(:import (java.time Instant))` is emitted too. Those specs are Java
/// packages that resolve to no repo file, but emitting them still BINDS
/// `Instant` to something external — which is what stops it misresolving to a
/// same-named local def.
fn collect_clj_imports(root: Node, bytes: &[u8], out: &mut Vec<ImportRef>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        // Unlike the other Clojure collectors this one must descend THROUGH
        // quotes: `(require '[app.db :as db])` quotes its spec, and that quote
        // is calling convention, not data.
        if n.kind() == "dis_expr" {
            continue;
        }
        if n.kind() == "list_lit" {
            let line = n.start_position().row as u32 + 1;
            match clj_head(n, bytes).as_deref() {
                Some("ns") => {
                    // payload: `ns`, the namespace name, then reference forms
                    // (a docstring or attribute map may sit between).
                    for form in clj_payload(n).into_iter().skip(2) {
                        if form.kind() != "list_lit" {
                            continue;
                        }
                        let Some(key) = clj_head_kwd(form, bytes) else {
                            continue;
                        };
                        let specs = clj_payload(form).into_iter().skip(1);
                        if CLJ_REQUIRE_KEYS.contains(&key.as_str()) {
                            for spec in specs {
                                push_clj_ns_spec(spec, bytes, "", line, out);
                            }
                        } else if key == ":import" {
                            for spec in specs {
                                push_clj_class_spec(spec, bytes, line, out);
                            }
                        }
                    }
                    continue;
                }
                Some(head) if CLJ_REQUIRE_HEADS.contains(&head) => {
                    for arg in clj_payload(n).into_iter().skip(1) {
                        let inner = if arg.kind() == "quoting_lit" {
                            clj_payload(arg).into_iter().next().unwrap_or(arg)
                        } else {
                            arg
                        };
                        push_clj_ns_spec(inner, bytes, "", line, out);
                    }
                    continue;
                }
                _ => {}
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// One `:require` spec: a bare `app.db`, a `[app.db :as db :refer [q]]`
/// vector, or a prefix list `[app [db :as db] [http :as http]]` (legal, and
/// still seen in older code). `prefix` is the accumulated namespace head when
/// recursing into a prefix list.
fn push_clj_ns_spec(node: Node, bytes: &[u8], prefix: &str, line: u32, out: &mut Vec<ImportRef>) {
    let qualify = |ns: &str| {
        if prefix.is_empty() {
            ns.to_string()
        } else {
            format!("{prefix}.{ns}")
        }
    };
    if node.kind() == "sym_lit" {
        if let Some(ns) = clj_sym_name(node, bytes).filter(|s| is_clj_name(s)) {
            out.push(ImportRef {
                spec: qualify(&ns),
                names: Vec::new(),
                line,
                alias: None,
                type_only: false,
            });
        }
        return;
    }
    // `:use` occasionally spells its specs as lists rather than vectors.
    if !matches!(node.kind(), "vec_lit" | "list_lit") {
        return;
    }
    let kids = clj_payload(node);
    let Some(&base) = kids.first().filter(|k| k.kind() == "sym_lit") else {
        return;
    };
    let Some(ns) = clj_sym_name(base, bytes).filter(|s| is_clj_name(s)) else {
        return;
    };
    // Prefix list: the head names a package and every sibling is its own spec.
    if kids.get(1).is_some_and(|k| matches!(k.kind(), "vec_lit" | "list_lit")) {
        let nested = qualify(&ns);
        for spec in &kids[1..] {
            push_clj_ns_spec(*spec, bytes, &nested, line, out);
        }
        return;
    }

    let mut alias = None;
    let mut names: Vec<ImportedSym> = Vec::new();
    let mut opts = kids[1..].iter();
    while let Some(key) = opts.next() {
        let Some(key) = clj_kwd_text(*key, bytes) else {
            continue;
        };
        let Some(&value) = opts.next() else {
            break;
        };
        match key.as_str() {
            // `:as-alias` binds a qualifier without loading — same binding
            // shape, so the same edge evidence.
            ":as" | ":as-alias" => {
                alias = clj_sym_name(value, bytes).filter(|s| is_clj_name(s));
            }
            // `:only` is `:use`'s spelling of `:refer`. `:refer :all` names no
            // individual symbol, so it stays whole-module evidence.
            ":refer" | ":only" => {
                if value.kind() == "vec_lit" {
                    names.extend(
                        clj_payload(value)
                            .into_iter()
                            .filter(|v| v.kind() == "sym_lit")
                            .filter_map(|v| clj_sym_name(v, bytes))
                            .filter(|s| is_clj_name(s))
                            .map(|s| ImportedSym::same(&s)),
                    );
                }
            }
            // `:rename {orig new}` — `new` is the local binding, `orig` the
            // name in the source namespace, exactly ImportedSym's split.
            ":rename" => {
                if value.kind() == "map_lit" {
                    let pairs = clj_payload(value);
                    for pair in pairs.chunks(2) {
                        if let [from, to] = pair {
                            if let (Some(name), Some(local)) =
                                (clj_sym_name(*from, bytes), clj_sym_name(*to, bytes))
                            {
                                names.push(ImportedSym { name, local });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out.push(ImportRef {
        spec: qualify(&ns),
        names,
        line,
        alias,
        type_only: false,
    });
}

/// One `:import` spec: `java.util.UUID` (fully-qualified class) or
/// `(java.time Instant Duration)` (package + classes). Split so the package is
/// the `spec` and the class names are the bindings.
fn push_clj_class_spec(node: Node, bytes: &[u8], line: u32, out: &mut Vec<ImportRef>) {
    if node.kind() == "sym_lit" {
        let Some(fqcn) = clj_sym_name(node, bytes).filter(|s| is_clj_name(s)) else {
            return;
        };
        let Some((pkg, class)) = fqcn.rsplit_once('.') else {
            return;
        };
        out.push(ImportRef {
            spec: pkg.to_string(),
            names: vec![ImportedSym::same(class)],
            line,
            alias: None,
            type_only: false,
        });
        return;
    }
    if !matches!(node.kind(), "vec_lit" | "list_lit") {
        return;
    }
    let kids = clj_payload(node);
    let Some(&pkg_node) = kids.first().filter(|k| k.kind() == "sym_lit") else {
        return;
    };
    let Some(pkg) = clj_sym_name(pkg_node, bytes) else {
        return;
    };
    let names: Vec<ImportedSym> = kids[1..]
        .iter()
        .filter(|k| k.kind() == "sym_lit")
        .filter_map(|k| clj_sym_name(*k, bytes))
        .filter(|s| is_clj_name(s))
        .map(|s| ImportedSym::same(&s))
        .collect();
    if names.is_empty() {
        return;
    }
    out.push(ImportRef {
        spec: pkg,
        names,
        line,
        alias: None,
        type_only: false,
    });
}

/// The text of a `kwd_lit`, colons included (`:as`, `::email`).
fn clj_kwd_text(node: Node, bytes: &[u8]) -> Option<String> {
    (node.kind() == "kwd_lit")
        .then(|| node.utf8_text(bytes).ok())
        .flatten()
        .map(|s| s.to_string())
}

/// String-named Clojure forms — `(testing "adds a user" …)`, `(facts "…" …)`:
/// any list whose head is a symbol and whose first argument is a literal
/// string, spanning the whole form. The Clojure counterpart of
/// [`collect_string_named_calls`], and just as broad: over-collection is
/// harmless because `test_blocks` is consulted only when a recorded symbol
/// matches no `defs` entry, and never feeds link resolution.
fn collect_clj_string_named_forms(root: Node, bytes: &[u8], out: &mut Vec<Def>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if clj_is_unevaluated(n.kind()) {
            continue;
        }
        if n.kind() == "list_lit" {
            let kids = clj_payload(n);
            if let (Some(&head), Some(&arg)) = (kids.first(), kids.get(1)) {
                if head.kind() == "sym_lit" && arg.kind() == "str_lit" {
                    if let Some(name) = clj_str_text(arg, bytes) {
                        let (start_line, end_line) = line_span(n);
                        out.push(Def {
                            name,
                            start_line,
                            end_line,
                            fields: Vec::new(),
                            is_data_shape: false,
                            ..Default::default()
                        });
                    }
                }
            }
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

// --- Clojure/EDN build descriptors ---
//
// A Clojure namespace maps to a FILE PATH under a declared source root, so
// resolving `app.db` needs `deps.edn`'s `:paths` (or the equivalent). EDN is
// Clojure data, so the grammar already bundled above reads it — no second
// parser, no regex over braces.

/// Parse Clojure/EDN source, for callers that walk the tree themselves.
fn parse_clj(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_clojure::LANGUAGE.into()).ok()?;
    parser.parse(source, None)
}

/// The value of a top-level EDN map key, read as a vector of strings:
/// `deps.edn`'s `{:paths ["src" "resources"]}` -> `["src", "resources"]`.
/// Empty when the key is absent or its value isn't a vector of literals.
pub fn edn_string_vec(source: &str, key: &str) -> Vec<String> {
    let Some(tree) = parse_clj(source) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let Some(map) = clj_payload(tree.root_node())
        .into_iter()
        .find(|n| n.kind() == "map_lit")
    else {
        return Vec::new();
    };
    let entries = clj_payload(map);
    for pair in entries.chunks(2) {
        if let [k, v] = pair {
            if clj_kwd_text(*k, bytes).as_deref() == Some(key) {
                return clj_string_vec(*v, bytes);
            }
        }
    }
    Vec::new()
}

/// The string literals of a `vec_lit`, in order.
fn clj_string_vec(node: Node, bytes: &[u8]) -> Vec<String> {
    if node.kind() != "vec_lit" {
        return Vec::new();
    }
    clj_payload(node)
        .into_iter()
        .filter(|n| n.kind() == "str_lit")
        .filter_map(|n| clj_str_text(n, bytes))
        .collect()
}

/// A Leiningen `project.clj` header: the declared project name and its
/// `:source-paths`. `(defproject my-group/my-app "0.1.0" :source-paths [..])`
/// -> `("my-app", ["src", ..])`. The name drops any group qualifier, matching
/// how the other ecosystems' display names read.
pub fn project_clj_header(source: &str) -> Option<(String, Vec<String>)> {
    let tree = parse_clj(source)?;
    let bytes = source.as_bytes();
    let form = clj_payload(tree.root_node())
        .into_iter()
        .find(|n| n.kind() == "list_lit" && clj_head(*n, bytes).as_deref() == Some("defproject"))?;
    let kids = clj_payload(form);
    let name = clj_sym_name(*kids.get(1)?, bytes).filter(|s| is_clj_name(s))?;

    // Options begin after the name and version; read every :source-paths-ish
    // key rather than assuming a fixed position.
    let mut paths = Vec::new();
    let mut opts = kids[2..].iter();
    while let Some(k) = opts.next() {
        let Some(k) = clj_kwd_text(*k, bytes) else {
            continue; // the version string, or a positional we don't read
        };
        let Some(&v) = opts.next() else { break };
        if k == ":source-paths" || k == ":test-paths" {
            paths.extend(clj_string_vec(v, bytes));
        }
    }
    Some((name, paths))
}

/// Conventional Clojure source roots, used when a unit declares none (or has
/// no build descriptor at all). Deliberately a fallback, not a default: a
/// declared `:paths` always wins.
pub const CLJ_FALLBACK_ROOTS: &[&str] = &[
    "src",
    "test",
    "src/main/clojure",
    "src/test/clojure",
    "src/main/cljs",
];

/// Candidate file paths for a Clojure namespace under one source root, most
/// preferred first. Namespace segments are dot-separated and `-` in a segment
/// is `_` on disk (the loader's munging), so `app.some-ns` lives at
/// `<root>/app/some_ns.clj`. `prefer` is the importing file's extension, so a
/// `.cljs` file resolves a `.cljs` sibling ahead of a `.clj` one.
pub fn clj_ns_candidates(root: &str, ns: &str, prefer: &str) -> Vec<String> {
    let path = ns.replace('.', "/").replace('-', "_");
    let base = if root.is_empty() {
        path
    } else {
        format!("{root}/{path}")
    };
    let exts: &[&str] = match prefer {
        "cljs" => &["cljs", "cljc", "clj"],
        "cljr" => &["cljr", "cljc", "clj"],
        _ => &["clj", "cljc", "cljs"],
    };
    exts.iter().map(|e| format!("{base}.{e}")).collect()
}

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
                        out.push(PathRef { segments, line, is_use: true });
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
        // A scoped path nested inside another is just a prefix of it — the
        // `crate::model` in `crate::model::Node`, whether that is a value path
        // or a type path (`scoped_type_identifier`). Only the outermost one is
        // a reference: its prefix names modules, and a module segment
        // resolved as a leaf would bind to any def that shares its name.
        let scoped = |k: &str| matches!(k, "scoped_identifier" | "scoped_type_identifier");
        if scoped(child.kind()) && !scoped(node.kind()) {
            if let Some(segments) = flatten_use(child, bytes).into_iter().next() {
                if segments.len() >= 2 {
                    out.push(PathRef {
                        segments,
                        line: child.start_position().row as u32 + 1,
                        is_use: false,
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
        "scoped_identifier" | "scoped_type_identifier" => {
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

// --- Python module imports (cross-file/container resolution input) ---

/// Walk the whole tree for `import a.b [as x]` and
/// `from [dots]mod import x [as y], …` — function-local imports are idiomatic
/// Python, so the walk is unconditional. A plain `import a.b` binds the module
/// object, not a symbol: file-level evidence only. `from m import *` likewise.
fn collect_py_imports(root: Node, bytes: &[u8], out: &mut Vec<ImportRef>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "import_statement" => {
                let line = n.start_position().row as u32 + 1;
                for child in named_children(n) {
                    let module = match child.kind() {
                        "dotted_name" => Some(child),
                        // `import a.b as x`: the module is the `name` field.
                        "aliased_import" => child.child_by_field_name("name"),
                        _ => None,
                    };
                    if let Some(spec) = module.and_then(|m| m.utf8_text(bytes).ok()) {
                        out.push(ImportRef {
                            spec: spec.to_string(),
                            names: Vec::new(),
                            line,
                            alias: None,
                            type_only: false,
                        });
                    }
                }
                continue;
            }
            "import_from_statement" => {
                let Some(spec) = n
                    .child_by_field_name("module_name")
                    .and_then(|m| m.utf8_text(bytes).ok())
                else {
                    continue;
                };
                let module_id = n.child_by_field_name("module_name").map(|m| m.id());
                let mut names = Vec::new();
                for child in named_children(n) {
                    if Some(child.id()) == module_id {
                        continue;
                    }
                    match child.kind() {
                        "dotted_name" => {
                            if let Ok(name) = child.utf8_text(bytes) {
                                names.push(ImportedSym::same(name));
                            }
                        }
                        "aliased_import" => {
                            let name = child
                                .child_by_field_name("name")
                                .and_then(|m| m.utf8_text(bytes).ok());
                            let local = child
                                .child_by_field_name("alias")
                                .and_then(|a| a.utf8_text(bytes).ok());
                            if let (Some(name), Some(local)) = (name, local) {
                                names.push(ImportedSym {
                                    name: name.to_string(),
                                    local: local.to_string(),
                                });
                            }
                        }
                        _ => {} // wildcard_import: file-level evidence only
                    }
                }
                out.push(ImportRef {
                    spec: spec.to_string(),
                    names,
                    line: n.start_position().row as u32 + 1,
                    alias: None,
                    type_only: false,
                });
                continue;
            }
            _ => {}
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

// --- TS/JS module imports (cross-file/container resolution input) ---

/// Walk the whole tree for the four import forms: `import … from "spec"`,
/// `export … from "spec"`, `require("spec")`, and dynamic `import("spec")`.
/// `import`/`export` statements are module-level, but `require`/`import()` can
/// sit anywhere, so the walk is unconditional.
fn collect_ts_imports(root: Node, bytes: &[u8], out: &mut Vec<ImportRef>) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "import_statement" => {
                if let Some(spec) = string_literal(n.child_by_field_name("source"), bytes) {
                    let mut names = Vec::new();
                    for child in named_children(n) {
                        if child.kind() == "import_clause" {
                            ts_import_clause_names(child, bytes, &mut names);
                        }
                    }
                    out.push(ImportRef {
                        spec,
                        names,
                        line: n.start_position().row as u32 + 1,
                        alias: None,
                        type_only: ts_type_only(n),
                    });
                }
                continue; // nothing else importable inside
            }
            "export_statement" => {
                // Only a RE-export (`export { x } from "m"`, `export * from "m"`)
                // is an import; a plain export wraps a declaration — descend.
                if let Some(spec) = string_literal(n.child_by_field_name("source"), bytes) {
                    let mut names = Vec::new();
                    for child in named_children(n) {
                        if child.kind() == "export_clause" {
                            for item in named_children(child) {
                                if item.kind() == "export_specifier" {
                                    // `export { a as b } from "m"`: the source
                                    // module's symbol is `a`; a re-export binds
                                    // no local name, so `local` mirrors it.
                                    if let Some(name) = field_text(item, "name", bytes) {
                                        names.push(ImportedSym::same(&name));
                                    }
                                }
                            }
                        }
                    }
                    out.push(ImportRef {
                        spec,
                        names,
                        line: n.start_position().row as u32 + 1,
                        alias: None,
                        type_only: ts_type_only(n),
                    });
                    continue;
                }
            }
            "call_expression" => {
                let is_import_call = n.child_by_field_name("function").is_some_and(|f| {
                    f.kind() == "import"
                        || (f.kind() == "identifier" && f.utf8_text(bytes) == Ok("require"))
                });
                if is_import_call {
                    if let Some(spec) = call_string_arg(n, bytes) {
                        out.push(ImportRef {
                            spec,
                            names: ts_binding_names(n, bytes),
                            line: n.start_position().row as u32 + 1,
                            alias: None,
                            type_only: false,
                        });
                    }
                }
            }
            _ => {}
        }
        for child in named_children(n) {
            stack.push(child);
        }
    }
}

/// Names bound by an import clause: the default-import local binding and each
/// named specifier (`{ a as b }` -> source `a`, local `b`). A namespace import
/// (`* as ns`) binds the whole module, not a symbol — contributes none.
/// Whether an `import` / `export … from` statement brings in types alone:
/// `import type …`, `export type … from`, or named specifiers that are each
/// `type X` with no default or namespace binding beside them.
fn ts_type_only(statement: Node) -> bool {
    if has_token(statement, "type") {
        return true;
    }
    let mut specifiers = 0;
    let mut types = 0;
    for child in named_children(statement) {
        let items: Vec<Node> = match child.kind() {
            "import_clause" => {
                let mut items = Vec::new();
                for part in named_children(child) {
                    match part.kind() {
                        "named_imports" => items.extend(named_children(part)),
                        // A default or namespace binding is a value.
                        _ => return false,
                    }
                }
                items
            }
            "export_clause" => named_children(child),
            _ => continue,
        };
        for item in items.into_iter().filter(|i| matches!(i.kind(), "import_specifier" | "export_specifier")) {
            specifiers += 1;
            if has_token(item, "type") {
                types += 1;
            }
        }
    }
    specifiers > 0 && specifiers == types
}

fn ts_import_clause_names(clause: Node, bytes: &[u8], names: &mut Vec<ImportedSym>) {
    for child in named_children(clause) {
        match child.kind() {
            "identifier" => {
                if let Ok(text) = child.utf8_text(bytes) {
                    names.push(ImportedSym::same(text));
                }
            }
            "named_imports" => {
                for item in named_children(child) {
                    if item.kind() == "import_specifier" {
                        if let Some(name) = field_text(item, "name", bytes) {
                            let local = field_text(item, "alias", bytes).unwrap_or_else(|| name.clone());
                            names.push(ImportedSym { name, local });
                        }
                    }
                }
            }
            _ => {} // namespace_import
        }
    }
}

/// Grammar-agnostic sweep for string-named call blocks (see
/// [`FileParse::test_blocks`]): every node whose kind names a call, whose
/// first argument is a literal string. Runner-agnostic on purpose — `it`,
/// `test`, `describe`, `t.Run`, `context` all reduce to the same shape, and
/// matching the shape means a new runner never needs a name added here.
fn collect_string_named_calls(root: Node, bytes: &[u8], out: &mut Vec<Def>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind().contains("call") {
            if let Some(name) = first_string_arg(node, bytes) {
                let (start_line, end_line) = line_span(node);
                out.push(Def {
                    name,
                    start_line,
                    end_line,
                    fields: Vec::new(),
                    is_data_shape: false,
                    ..Default::default()
                });
            }
        }
        for child in named_children(node) {
            stack.push(child);
        }
    }
}

/// The literal content of a call's first argument when it is a plain string,
/// across grammars: the argument list is the `arguments` field (or the named
/// child whose kind says "argument"), and a string-kinded first argument has
/// its quotes trimmed. Computed strings (templates with interpolation,
/// concatenations) don't reduce to a stable name and yield `None`.
fn first_string_arg(call: Node, bytes: &[u8]) -> Option<String> {
    let args = call
        .child_by_field_name("arguments")
        .or_else(|| named_children(call).into_iter().find(|n| n.kind().contains("argument")))?;
    let first = named_children(args).into_iter().next()?;
    if !first.kind().contains("string") {
        return None;
    }
    let raw = first.utf8_text(bytes).ok()?;
    // Trim one matching quote pair; reject anything still holding structure
    // (an interpolated template keeps `${` after trimming).
    let inner = raw
        .strip_prefix(['"', '\'', '`'])
        .and_then(|s| s.strip_suffix(['"', '\'', '`']))?;
    (!inner.is_empty() && !inner.contains("${")).then(|| inner.to_string())
}

/// The literal text of a `string` node, or `None` for anything computed
/// (template strings, identifiers, concatenations).
fn string_literal(node: Option<Node>, bytes: &[u8]) -> Option<String> {
    let node = node?;
    if node.kind() != "string" {
        return None;
    }
    let text: String = named_children(node)
        .into_iter()
        .filter(|c| c.kind() == "string_fragment")
        .filter_map(|c| c.utf8_text(bytes).ok())
        .collect();
    (!text.is_empty()).then_some(text)
}

/// First argument of a call when it is a literal string (`require("m")`,
/// `import("m", opts)`).
fn call_string_arg(call: Node, bytes: &[u8]) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    string_literal(named_children(args).into_iter().next(), bytes)
}

/// Names bound by a `require`/dynamic-import call's enclosing declarator:
/// `const { a, b: c } = require("m")` -> `[a/a, b/c]`, `const x = require("m")`
/// -> `[x/x]`, unwrapping one `await` for `const { a } = await import("m")`.
/// Anything else (bare call, member access) binds no symbol names.
fn ts_binding_names(call: Node, bytes: &[u8]) -> Vec<ImportedSym> {
    let mut value = call;
    if let Some(p) = value.parent() {
        if p.kind() == "await_expression" {
            value = p;
        }
    }
    let Some(declarator) = value.parent().filter(|p| p.kind() == "variable_declarator") else {
        return Vec::new();
    };
    if declarator.child_by_field_name("value").map(|v| v.id()) != Some(value.id()) {
        return Vec::new();
    }
    let Some(name_node) = declarator.child_by_field_name("name") else {
        return Vec::new();
    };
    match name_node.kind() {
        "identifier" => name_node
            .utf8_text(bytes)
            .ok()
            .map(|s| vec![ImportedSym::same(s)])
            .unwrap_or_default(),
        "object_pattern" => named_children(name_node)
            .into_iter()
            .filter_map(|c| match c.kind() {
                "shorthand_property_identifier_pattern" => {
                    c.utf8_text(bytes).ok().map(ImportedSym::same)
                }
                // `{ a: local }`: source-side name `a`, bound as `local`.
                "pair_pattern" => {
                    let name = field_text(c, "key", bytes)?;
                    let local = field_text(c, "value", bytes).unwrap_or_else(|| name.clone());
                    Some(ImportedSym { name, local })
                }
                // `{ a = fallback }`: source-side name `a`.
                "object_assignment_pattern" => c
                    .child_by_field_name("left")
                    .and_then(|l| l.utf8_text(bytes).ok())
                    .map(ImportedSym::same),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(parse: &FileParse) -> Vec<&str> {
        parse.defs.iter().map(|d| d.name.as_str()).collect()
    }

    /// When a TypeScript import or re-export brings in types alone — `import type`, `export type … from`, or named specifiers that are every one `type X` — it is type-only, and one binding a value beside them is not.
    #[test]
    fn typescript_type_only_imports_are_marked() {
        let src = "import type { A } from \"./a\";\nimport { type B, type C } from \"./b\";\nimport { type D, e } from \"./d\";\nimport F, { type G } from \"./f\";\nimport * as h from \"./h\";\nexport type { I } from \"./i\";\nexport { j } from \"./j\";\n";
        let p = parse_file(Path::new("src/x.ts"), src).unwrap();
        let mut got: Vec<(&str, bool)> = p.imports.iter().map(|i| (i.spec.as_str(), i.type_only)).collect();
        got.sort();
        assert_eq!(
            got,
            vec![("./a", true), ("./b", true), ("./d", false), ("./f", false), ("./h", false), ("./i", true), ("./j", false)]
        );
    }

    fn def<'p>(parse: &'p FileParse, name: &str) -> &'p Def {
        parse.defs.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("no def {name}: {:?}", names(parse)))
    }

    /// (name, type, doc, optional) of each member, for comparing.
    fn members(d: &Def) -> Vec<(&str, Option<&str>, Option<&str>, bool)> {
        d.members.iter().map(|m| (m.name.as_str(), m.ty.as_deref(), m.doc.as_deref(), m.optional)).collect()
    }

    /// When a Rust item has `///` lines or a `/** */` block above it, attributes between them, its doc is that text, one paragraph per blank-line break.
    #[test]
    fn a_rust_items_doc_comment_is_its_doc() {
        let src = "/// Holds a draft.\n///\n/// Saved on every edit;\n/// never lost.\n///\n/// ```\n/// let d = Draft::new();\n/// ```\n#[derive(Debug)]\npub struct Draft;\n\n/** Saves it.\n * Twice. */\npub fn save() {}\n\n// not a doc\nfn bare() {}\n\nimpl Draft {\n    /// Makes one.\n    pub fn new() -> Self { Draft }\n}\n";
        let p = parse_file(Path::new("src/draft.rs"), src).unwrap();
        assert_eq!(def(&p, "Draft").doc.as_deref(), Some("Holds a draft.\n\nSaved on every edit; never lost."));
        assert_eq!(def(&p, "save").doc.as_deref(), Some("Saves it. Twice."));
        assert_eq!(def(&p, "bare").doc, None);
        assert_eq!(def(&p, "new").doc.as_deref(), Some("Makes one."));
    }

    /// When a Rust struct or enum is a data shape, each field carries its type and doc comment, and each variant its doc and its payload as its type.
    #[test]
    fn rust_fields_and_variants_carry_type_and_doc() {
        let src = "pub struct Draft {\n    /// The text, as typed.\n    #[serde(default)]\n    pub text: String,\n    pub tags: Vec<\n        String,\n    >,\n}\n\npub enum Event {\n    /// Text arrived.\n    Message { text: String },\n    Stopped(u32, String),\n    Done,\n}\n";
        let p = parse_file(Path::new("src/draft.rs"), src).unwrap();
        let draft = def(&p, "Draft");
        assert_eq!(draft.fields, vec!["text", "tags"]);
        assert_eq!(
            members(draft),
            vec![("text", Some("String"), Some("The text, as typed."), false), ("tags", Some("Vec< String >"), None, false)]
        );
        assert_eq!(
            members(def(&p, "Event")),
            vec![
                ("Message", Some("{ text: String }"), Some("Text arrived."), false),
                ("Stopped", Some("(u32, String)"), None, false),
                ("Done", None, None, false),
            ]
        );
    }

    /// When a TypeScript declaration or its `export` has a JSDoc directly above it, that is its doc, tags dropped, and an interface's members carry their type annotation, JSDoc and optional mark.
    #[test]
    fn typescript_jsdoc_and_member_types() {
        let src = "/** @file The model types. */\n\n/**\n * A draft being written.\n * @see Store\n */\nexport interface Draft {\n  /** The text, as typed. */\n  text: string;\n  tags?: string[];\n  onSave: (d: Draft) => void;\n}\n\n/** Saves it. */\nfunction save() {}\n\n/** Detached. */\n\nexport const limit = 3;\n\nexport class Store {\n  /** How many. */\n  count?: number = 0;\n}\n";
        let p = parse_file(Path::new("src/model.ts"), src).unwrap();
        let draft = def(&p, "Draft");
        assert_eq!(draft.doc.as_deref(), Some("A draft being written."));
        assert_eq!(
            members(draft),
            vec![
                ("text", Some("string"), Some("The text, as typed."), false),
                ("tags", Some("string[]"), None, true),
                ("onSave", Some("(d: Draft) => void"), None, false),
            ]
        );
        assert_eq!(draft.fields, vec!["text", "tags", "onSave"]);
        assert_eq!(def(&p, "save").doc.as_deref(), Some("Saves it."));
        assert_eq!(def(&p, "limit").doc, None, "a JSDoc a blank line away documents nothing");
        assert_eq!(members(def(&p, "Store")), vec![("count", Some("number"), Some("How many."), true)]);
    }

    /// When a PascalCase function in a `.tsx` file is a React component, its props are its members: an inline object type, a named interface or alias of the same file, or the names it destructures; it is no data shape.
    #[test]
    fn react_component_props_are_its_members() {
        let src = "interface CardProps {\n  /** Shown on top. */\n  title: string;\n  size?: number;\n}\ntype Wrapped = PropsWithChildren<CardProps>;\n\n/** A card. */\nexport function Card({ title }: CardProps) { return <div>{title}</div>; }\n\nexport const Badge = ({ label, tone = \"info\" }: { label: string; tone?: string }) => <span>{label}</span>;\n\nexport const Pill = memo(({ text, onClose: close }) => <i>{text}</i>);\n\nexport const Shell: FC<Wrapped> = (props) => <main />;\n\nexport function helper(x: number) { return x; }\nexport const API_URL = \"/api\";\n";
        let p = parse_file(Path::new("src/ui/Card.tsx"), src).unwrap();
        let card = def(&p, "Card");
        assert!(card.is_component && !card.is_data_shape && card.fields.is_empty());
        assert_eq!(card.doc.as_deref(), Some("A card."));
        assert_eq!(members(card), vec![("title", Some("string"), Some("Shown on top."), false), ("size", Some("number"), None, true)]);
        assert_eq!(
            members(def(&p, "Badge")),
            vec![("label", Some("string"), None, false), ("tone", Some("string"), None, true)]
        );
        let pill: Vec<&str> = def(&p, "Pill").members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(pill, vec!["text", "onClose"]);
        let shell: Vec<&str> = def(&p, "Shell").members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(shell, vec!["title", "size"]);
        assert!(!def(&p, "helper").is_component && !def(&p, "API_URL").is_component);
        let ts = parse_file(Path::new("src/ui/card.ts"), "export function Card(p: { a: string }) { return p.a; }\n").unwrap();
        assert!(!def(&ts, "Card").is_component, "only .tsx/.jsx files hold components");
    }

    /// When a Go declaration has a comment block directly above it, that is its doc, and a struct's fields carry their type and doc.
    #[test]
    fn go_docs_and_struct_fields() {
        let src = "package draft\n\n// Draft is what the user types.\n// It is saved often.\ntype Draft struct {\n\t// Text as typed.\n\tText string\n\tX, Y int\n}\n\n//go:generate stringer\n// Save stores it.\nfunc Save() {}\n";
        let p = parse_file(Path::new("draft.go"), src).unwrap();
        let draft = def(&p, "Draft");
        assert_eq!(draft.doc.as_deref(), Some("Draft is what the user types. It is saved often."));
        assert_eq!(
            members(draft),
            vec![("Text", Some("string"), Some("Text as typed."), false), ("X", Some("int"), None, false), ("Y", Some("int"), None, false)]
        );
        assert_eq!(def(&p, "Save").doc.as_deref(), Some("Save stores it."));
    }

    /// When a Python function or class has a docstring it is its doc, and a dataclass is a data shape whose annotated fields carry their type and the string under them.
    #[test]
    fn python_docstrings_and_dataclass_fields() {
        let src = "from dataclasses import dataclass\n\n@dataclass(frozen=True)\nclass Draft:\n    \"\"\"A draft.\n\n    Saved often.\n    \"\"\"\n    text: str = \"\"\n    \"\"\"As typed.\"\"\"\n    tags: list[str]\n\n    def save(self):\n        \"\"\"Stores it.\"\"\"\n\nclass Plain:\n    x = 1\n";
        let p = parse_file(Path::new("draft.py"), src).unwrap();
        let draft = def(&p, "Draft");
        assert!(draft.is_data_shape);
        assert_eq!(draft.doc.as_deref(), Some("A draft.\n\nSaved often."));
        assert_eq!(members(draft), vec![("text", Some("str"), Some("As typed."), false), ("tags", Some("list[str]"), None, false)]);
        assert_eq!(def(&p, "save").doc.as_deref(), Some("Stores it."));
        assert!(!def(&p, "Plain").is_data_shape);
    }

    /// When a `.vue` single-file component is parsed, makes it one definition named after its file, reading its script block's imports and identifiers at their lines in the `.vue` file.
    #[test]
    fn vue_sfc_is_one_component_named_after_its_file() {
        let src = "<template>\n  <div>{{ title }}<Avatar :user=\"user\" /></div>\n</template>\n\n<script setup lang=\"ts\">\nimport Avatar from \"./Avatar.vue\";\nimport { formatName } from \"../lib/names\";\ndefineProps<{ title: string }>();\n</script>\n\n<style scoped>\n.x { color: red; }\n</style>\n";
        let p = parse_file(Path::new("src/components/user-card.vue"), src).unwrap();
        assert_eq!(names(&p), vec!["UserCard"]);
        assert_eq!((p.defs[0].start_line, p.defs[0].end_line), (1, 13));
        assert!(!p.defs[0].is_data_shape);
        let mut specs: Vec<&str> = p.imports.iter().map(|i| i.spec.as_str()).collect();
        specs.sort();
        assert_eq!(specs, vec!["../lib/names", "./Avatar.vue"]);
        // The import sits on line 6 of the .vue file, not line 1 of the script.
        let avatar = p.imports.iter().find(|i| i.spec == "./Avatar.vue").unwrap();
        assert_eq!(avatar.line, 6);
        assert!(p.idents.iter().any(|i| i.name == "formatName"));
        // Template-only words are not identifiers — the template is not code.
        assert!(!p.idents.iter().any(|i| i.name == "template"));
    }

    /// No script block at all (a pure-template SFC) still yields the component.
    #[test]
    fn vue_sfc_without_script_still_yields_the_component() {
        let p = parse_file(Path::new("Banner.vue"), "<template><p>hi</p></template>\n").unwrap();
        assert_eq!(names(&p), vec!["Banner"]);
        assert!(p.imports.is_empty() && p.idents.is_empty());
    }

    #[test]
    fn ts_test_blocks_by_name_string() {
        let src = "describe(\"webhook verify\", () => {\n  it(\"rejects an unsigned webhook\", async () => {\n    expect(res.status).toBe(403);\n  });\n  test.skip(\"echoes hub.challenge\", () => {});\n});\n";
        let p = parse_file(Path::new("f.spec.ts"), src).unwrap();
        let block = |name: &str| p.test_blocks.iter().find(|d| d.name == name);
        assert_eq!(
            block("rejects an unsigned webhook").map(|d| (d.start_line, d.end_line)),
            Some((2, 4)),
        );
        assert_eq!(block("webhook verify").map(|d| (d.start_line, d.end_line)), Some((1, 6)));
        assert!(block("echoes hub.challenge").is_some(), "member-call runners count too");
        // String-named blocks stay out of `defs` — link resolution never sees them.
        assert!(!names(&p).contains(&"rejects an unsigned webhook"));
    }

    #[test]
    fn go_subtest_blocks_by_name_string() {
        let src = "package p\n\nfunc TestVerify(t *testing.T) {\n\tt.Run(\"rejects bad token\", func(t *testing.T) {\n\t})\n}\n";
        let p = parse_file(Path::new("f_test.go"), src).unwrap();
        assert!(p.test_blocks.iter().any(|d| d.name == "rejects bad token"));
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
        // Segment-path capture is Rust-only; TS/JS speaks `imports` instead.
        let p = parse_file(Path::new("f.ts"), "import {x} from './y';\n").unwrap();
        assert!(p.paths.is_empty());
        assert!(!p.imports.is_empty());
        // And the converse: Rust never emits `imports`.
        let r = parse_file(Path::new("f.rs"), "use scryer_core::drift;\n").unwrap();
        assert!(r.imports.is_empty());
    }

    /// An import appears in the capture iff spec + (name, local) pairs match,
    /// regardless of line.
    fn has_import(p: &FileParse, spec: &str, names: &[(&str, &str)]) -> bool {
        let names: Vec<ImportedSym> = names
            .iter()
            .map(|(name, local)| ImportedSym {
                name: name.to_string(),
                local: local.to_string(),
            })
            .collect();
        p.imports.iter().any(|i| i.spec == spec && i.names == names)
    }

    #[test]
    fn ts_esm_imports_captured() {
        let src = r#"
import Button, { Card as C, type Kind } from "@acme/ui";
import * as helpers from "./helpers";
import "./styles.css";
export { fmtDate as fd } from "../lib/dates";
export * from "./reexports";
"#;
        let p = parse_file(Path::new("f.ts"), src).unwrap();
        // Default binding + named specifiers; `as` renames keep the SOURCE name
        // as the edge target and the alias as the local binding.
        assert!(has_import(
            &p,
            "@acme/ui",
            &[("Button", "Button"), ("Card", "C"), ("Kind", "Kind")]
        ));
        // Namespace and side-effect imports: file-level evidence, no names.
        assert!(has_import(&p, "./helpers", &[]));
        assert!(has_import(&p, "./styles.css", &[]));
        // Re-exports are imports of the source module; no local binding exists,
        // so local mirrors the source name.
        assert!(has_import(&p, "../lib/dates", &[("fmtDate", "fmtDate")]));
        assert!(has_import(&p, "./reexports", &[]));
    }

    #[test]
    fn ts_require_and_dynamic_import_captured() {
        let src = r#"
const { readFile, stat: statFn } = require("node:fs/promises");
const legacy = require("./legacy");
require("./side-effect");
async function load() {
  const { widget } = await import("./lazy");
}
"#;
        let p = parse_file(Path::new("f.js"), src).unwrap();
        // Destructured require: source-side name, destructure rename kept as local.
        assert!(has_import(
            &p,
            "node:fs/promises",
            &[("readFile", "readFile"), ("stat", "statFn")]
        ));
        // Whole-module binding: the local name, resolved unique-or-skip later.
        assert!(has_import(&p, "./legacy", &[("legacy", "legacy")]));
        assert!(has_import(&p, "./side-effect", &[]));
        assert!(has_import(&p, "./lazy", &[("widget", "widget")]));
    }

    #[test]
    fn go_defs_and_fields() {
        let src = r#"
package db

const MaxConns = 10

var registry = map[string]int{}

type Store struct {
	Conn *sql.DB
	Name, Kind string
}

type Reader interface {
	Read(p []byte) (int, error)
	Close() error
}

type (
	ID   int64
	Pair struct{ A, B int }
)

func Connect(dsn string) (*Store, error) { return nil, nil }

func (s *Store) Close() error { return nil }
"#;
        let p = parse_file(Path::new("f.go"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"MaxConns"));
        assert!(n.contains(&"registry"));
        assert!(n.contains(&"Store"), "structs are symbols (audit theme 2)");
        assert!(n.contains(&"Reader"), "interfaces are symbols");
        assert!(n.contains(&"ID"), "grouped type specs each yield a def");
        assert!(n.contains(&"Pair"));
        assert!(n.contains(&"Connect"));
        assert!(n.contains(&"Close"), "receiver methods are symbols");
        // Interface method specs must NOT mint their own defs (the old
        // fallback's `method_elem` bug): exactly one `Close` — the method.
        assert_eq!(
            p.defs.iter().filter(|d| d.name == "Close").count(),
            1,
            "interface method specs are not defs"
        );
        assert!(!n.contains(&"Read"), "interface method specs are not defs");

        let store = p.defs.iter().find(|d| d.name == "Store").unwrap();
        assert!(store.is_data_shape);
        assert_eq!(store.fields, vec!["Conn", "Name", "Kind"]);
        let reader = p.defs.iter().find(|d| d.name == "Reader").unwrap();
        assert!(!reader.is_data_shape, "an interface is behavioral");
    }

    #[test]
    fn go_imports_and_qualified_paths_captured() {
        let src = r#"
package main

import (
	"fmt"
	database "github.com/acme/proj/internal/db"
	_ "github.com/lib/pq"
	"github.com/acme/proj/pkg/api"
)

func main() {
	s, _ := database.Connect("dsn")
	var h api.Handler
	fmt.Println(s, h)
}
"#;
        let p = parse_file(Path::new("f.go"), src).unwrap();
        // Aliased import: the alias is the binding.
        assert!(has_import(
            &p,
            "github.com/acme/proj/internal/db",
            &[("database", "database")]
        ));
        // Default binding: the path's last segment.
        assert!(has_import(&p, "github.com/acme/proj/pkg/api", &[("api", "api")]));
        assert!(has_import(&p, "fmt", &[("fmt", "fmt")]));
        // Blank import binds nothing.
        assert!(has_import(&p, "github.com/lib/pq", &[]));
        // Qualified references: selector at the call site, qualified type.
        assert!(p
            .paths
            .iter()
            .any(|pr| pr.segments == ["database", "Connect"] && pr.line == 12));
        assert!(p.paths.iter().any(|pr| pr.segments == ["api", "Handler"]));
        assert!(p.paths.iter().any(|pr| pr.segments == ["fmt", "Println"]));
    }

    #[test]
    fn c_defs_and_fields() {
        let src = r#"
#include <stdio.h>

#define MAX 10

struct Point {
    int x, y;
    char *label;
};

enum Color { RED, GREEN };

typedef struct { int a; } Wrapped;

static int counter;

int add(int a, int b) {
    struct Point local;
    return a + b;
}

char *render(struct Point *p) {
    return p->label;
}
"#;
        let p = parse_file(Path::new("f.c"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"add"), "functions are symbols (audit theme 2)");
        assert!(n.contains(&"render"), "pointer-returning functions too");
        assert!(n.contains(&"Point"));
        assert!(n.contains(&"Color"));
        assert!(n.contains(&"Wrapped"), "typedef names are symbols");
        // `struct Point local;` inside a body must NOT mint a phantom def, and
        // locals are not symbols.
        assert_eq!(
            p.defs.iter().filter(|d| d.name == "Point").count(),
            1,
            "a struct USE site is not a definition"
        );
        assert!(!n.contains(&"local"));

        let point = p.defs.iter().find(|d| d.name == "Point").unwrap();
        assert!(point.is_data_shape);
        assert_eq!(point.fields, vec!["x", "y", "label"]);
        let color = p.defs.iter().find(|d| d.name == "Color").unwrap();
        assert_eq!(color.fields, vec!["RED", "GREEN"]);
    }

    #[test]
    fn cpp_defs() {
        let src = r#"
namespace app {

class Widget {
public:
    int size;
    void draw() { }
private:
    int state_;
};

int Widget_helper() { return 1; }

}

void app::Widget_out_of_line() { }
"#;
        let p = parse_file(Path::new("f.cpp"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"Widget"), "classes inside namespaces");
        assert!(n.contains(&"draw"), "inline methods are symbols");
        assert!(n.contains(&"Widget_helper"));
        assert!(
            n.contains(&"Widget_out_of_line"),
            "qualified out-of-line defs take the last name segment"
        );
        let widget = p.defs.iter().find(|d| d.name == "Widget").unwrap();
        assert_eq!(widget.fields, vec!["size", "state_"]);
    }

    #[test]
    fn dollar_and_unicode_identifiers_kept() {
        let src = "export const $store = 1;\nexport function übersetzen() { return 1; }\n";
        let p = parse_file(Path::new("f.ts"), src).unwrap();
        let n = names(&p);
        assert!(n.contains(&"$store"), "JS $-names are real symbols");
        assert!(n.contains(&"übersetzen"), "unicode identifiers too");
    }

    #[test]
    fn py_imports_captured() {
        let src = r#"
import os
import app.util as u
from app.dates import fmt_date, parse as parse_date
from . import sibling
from ..pkg.mod import Thing
from app.legacy import *

def local_scope():
    from app.lazy import widget
    return widget
"#;
        let p = parse_file(Path::new("f.py"), src).unwrap();
        // Plain `import` binds the module object: file-level evidence only.
        assert!(has_import(&p, "os", &[]));
        assert!(has_import(&p, "app.util", &[]));
        // `from … import` names symbols; `as` keeps the source name + local.
        assert!(has_import(
            &p,
            "app.dates",
            &[("fmt_date", "fmt_date"), ("parse", "parse_date")]
        ));
        // Relative specs keep their dots for the resolver.
        assert!(has_import(&p, ".", &[("sibling", "sibling")]));
        assert!(has_import(&p, "..pkg.mod", &[("Thing", "Thing")]));
        // Wildcard: file-level only.
        assert!(has_import(&p, "app.legacy", &[]));
        // Function-local imports are captured too.
        assert!(has_import(&p, "app.lazy", &[("widget", "widget")]));
    }

    #[test]
    fn ts_computed_specs_skipped() {
        // A computed module spec has no unambiguous target: no capture.
        let src = r#"
const name = "./plugin";
const a = require(name);
const b = require(`./gen/${name}`);
async function f(p: string) { return import(p); }
"#;
        let p = parse_file(Path::new("f.ts"), src).unwrap();
        assert!(p.imports.is_empty(), "computed specs must not be captured");
    }

    #[test]
    fn ts_import_lines_attributed() {
        let src = "import { a } from \"./x\";\n\nconst y = require(\"./z\");\n";
        let p = parse_file(Path::new("f.ts"), src).unwrap();
        assert_eq!(
            p.imports.iter().find(|i| i.spec == "./x").unwrap().line,
            1
        );
        assert_eq!(
            p.imports.iter().find(|i| i.spec == "./z").unwrap().line,
            3
        );
    }

    /// Extensions whose real declared imports are followed read `full`;
    /// grammar-backed languages resolved only by bare-identifier coincidence
    /// read `nameHeuristic`; no grammar, no tier.
    #[test]
    fn resolution_tier_classifies_full_and_name_heuristic() {
        for ext in ["rs", "ts", "tsx", "py", "go", "clj", "cljs", "cljc", "cs"] {
            assert_eq!(import_resolution_tier(ext), Some("full"), "{ext}");
        }
        for ext in ["c", "cpp", "h"] {
            assert_eq!(import_resolution_tier(ext), Some("nameHeuristic"), "{ext}");
        }
        assert_eq!(import_resolution_tier("xyz"), None);
    }

    // --- Clojure ---

    /// The nasty cases all at once: `def…` forms of every arity, metadata
    /// before the name, a user-defined defining macro, a spec keyword name,
    /// and the three ways Clojure holds code that is NOT a definition.
    const CLJ_SRC: &str = r#"(ns app.core
  "Docstring."
  (:require [app.db :as db]
            [clojure.string :as str :refer [join trim]]
            [app.util :refer :all]
            app.plain
            [app [http :as http] [mail :as mail]]
            [app.legacy :as leg :rename {old-name new-name}])
  (:import (java.time Instant Duration)
           java.util.UUID))

(def ^:private max-retries 3)

(defn fetch-user
  "Look up a user."
  [id]
  (db/query {:id id}))

(defn- helper [x] (inc x))

(defrecord User [id email created-at]
  Store
  (put! [this k v] nil))

(defprotocol Store
  "A place to put things."
  (put! [this k v])
  (get! [this k]))

(defmulti render :kind)
(defmethod render :html [x] (str/join x))
(defmethod render :text [x] (str x))

(defonce cache (atom {}))

(s/def ::email string?)

(defroutes app-routes
  (GET "/" [] "hi"))

(comment
  (defn scratch [] 1))

'(defn quoted [] 1)

#_(defn discarded [] 1)

(deftest fetch-user-test
  (testing "returns a user"
    (is (= 1 1))))
"#;

    fn clj_parse() -> FileParse {
        parse_file(Path::new("app/core.clj"), CLJ_SRC).expect("clojure parses")
    }

    /// When a list's head names a defining form, record the following symbol as a definition, ignoring quoted, discarded and commented code
    #[test]
    fn clojure_def_forms_collected() {
        let p = clj_parse();
        let names: Vec<&str> = p.defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "max-retries",     // def, with ^:private metadata before the name
                "fetch-user",      // defn with a docstring
                "helper",          // defn-
                "User",            // defrecord
                "Store",           // defprotocol
                "render",          // defmulti
                "render",          // defmethod :html
                "render",          // defmethod :text
                "cache",           // defonce
                "::email",         // s/def, named by a keyword
                "app-routes",      // defroutes: a user-defined defining macro
                "fetch-user-test", // deftest
            ]
        );
    }

    /// `(comment …)`, `'quoted` and `#_discarded` forms hold readable code
    /// that defines nothing — the ambiguity that keeps `defn` out of the
    /// grammar in the first place.
    #[test]
    fn clojure_unevaluated_forms_define_nothing() {
        let p = clj_parse();
        for ghost in ["scratch", "quoted", "discarded"] {
            assert!(
                !p.defs.iter().any(|d| d.name == ghost),
                "{ghost} is not a definition"
            );
        }
    }

    /// A forward declaration has no body: minting it would add a same-named
    /// peer at a span holding no definition.
    #[test]
    fn clojure_declare_is_not_a_def() {
        let src = "(declare later-fn)\n\n(defn later-fn [] 1)\n";
        let p = parse_file(Path::new("a.clj"), src).unwrap();
        assert_eq!(p.defs.len(), 1);
        assert_eq!((p.defs[0].start_line, p.defs[0].end_line), (3, 3));
    }

    #[test]
    fn clojure_def_spans_whole_form() {
        let p = clj_parse();
        let f = p.defs.iter().find(|d| d.name == "fetch-user").unwrap();
        assert_eq!((f.start_line, f.end_line), (14, 17));
    }

    #[test]
    fn clojure_data_shapes_carry_fields() {
        let p = clj_parse();
        let rec = p.defs.iter().find(|d| d.name == "User").unwrap();
        assert!(rec.is_data_shape);
        // Only the field vector — not the params of the inline `put!` impl.
        assert_eq!(rec.fields, vec!["id", "email", "created-at"]);

        // A protocol is an interface: its methods are its declared members.
        let proto = p.defs.iter().find(|d| d.name == "Store").unwrap();
        assert!(proto.is_data_shape);
        assert_eq!(proto.fields, vec!["put!", "get!"]);

        let f = p.defs.iter().find(|d| d.name == "fetch-user").unwrap();
        assert!(!f.is_data_shape && f.fields.is_empty());
    }

    /// When a namespace declaration lists requires, record each required namespace with its alias and referred names
    #[test]
    fn clojure_ns_requires_become_imports() {
        let p = clj_parse();
        let by_spec = |spec: &str| p.imports.iter().find(|i| i.spec == spec).unwrap();

        // `:as` binds a qualifier, not symbols.
        let db = by_spec("app.db");
        assert_eq!(db.alias.as_deref(), Some("db"));
        assert!(db.names.is_empty());

        // `:as` and `:refer` together.
        let s = by_spec("clojure.string");
        assert_eq!(s.alias.as_deref(), Some("str"));
        assert_eq!(
            s.names.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["join", "trim"]
        );

        // `:refer :all` names no symbol — whole-namespace evidence only.
        let all = by_spec("app.util");
        assert!(all.names.is_empty() && all.alias.is_none());

        // A bare symbol spec.
        assert!(by_spec("app.plain").names.is_empty());

        // Prefix list: `[app [http :as http] …]` -> `app.http`, `app.mail`.
        assert_eq!(by_spec("app.http").alias.as_deref(), Some("http"));
        assert_eq!(by_spec("app.mail").alias.as_deref(), Some("mail"));

        // `:rename` splits source name from local binding.
        let leg = by_spec("app.legacy");
        assert_eq!(
            leg.names,
            vec![ImportedSym {
                name: "old-name".to_string(),
                local: "new-name".to_string(),
            }]
        );
    }

    /// `:import` targets Java packages that resolve to no repo file, but
    /// emitting them still binds `Instant`/`UUID` to something external so
    /// they can't misresolve to a same-named local def.
    #[test]
    fn clojure_java_imports_split_package_from_class() {
        let p = clj_parse();
        let time = p.imports.iter().find(|i| i.spec == "java.time").unwrap();
        assert_eq!(
            time.names.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            vec!["Instant", "Duration"]
        );
        let util = p.imports.iter().find(|i| i.spec == "java.util").unwrap();
        assert_eq!(util.names[0].name, "UUID");
    }

    #[test]
    fn clojure_top_level_require_is_read() {
        let src = "(require '[app.db :as db])\n(use 'app.legacy)\n";
        let p = parse_file(Path::new("a.clj"), src).unwrap();
        let db = p.imports.iter().find(|i| i.spec == "app.db").unwrap();
        assert_eq!(db.alias.as_deref(), Some("db"));
        assert!(p.imports.iter().any(|i| i.spec == "app.legacy"));
    }

    /// A qualified call site is exact evidence: the alias was bound by this
    /// file's own `ns` form, so `db/query` is resolvable where bare `query`
    /// would only be a coincidence.
    #[test]
    fn clojure_qualified_calls_become_paths() {
        let p = clj_parse();
        assert!(p
            .paths
            .iter()
            .any(|r| r.segments == vec!["db".to_string(), "query".to_string()] && r.line == 17));
        // The leaf of a QUALIFIED symbol is not also an ident: letting
        // `db/query` feed the bare-name scopes would let an external
        // `lib/query` misresolve to a same-named local def.
        assert!(!p.idents.iter().any(|i| i.name == "query"));
        // Unqualified symbols still are — that is how a same-namespace
        // reference resolves.
        assert!(p.idents.iter().any(|i| i.name == "atom"));
    }

    #[test]
    fn clojure_testing_blocks_anchor_by_name() {
        let p = clj_parse();
        let t = p
            .test_blocks
            .iter()
            .find(|d| d.name == "returns a user")
            .expect("testing block");
        assert_eq!(t.start_line, 49);
    }
}
