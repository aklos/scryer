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

/// Import-resolution coverage tier of a source extension: `full` when the
/// link audit sees the language's real declared imports (Rust paths, TS/JS
/// imports + tsconfig aliases, Python module paths, Go module paths),
/// `nameHeuristic` when it only has bare-identifier coincidence within a
/// container — where a real cross-container link can audit as asserted-only.
/// `None` for extensions with no grammar. Health reports this so the audit's
/// verdict is calibrated instead of silently overstated.
pub fn import_resolution_tier(ext: &str) -> Option<&'static str> {
    language_for_ext(ext)?;
    Some(match family_for_ext(ext) {
        Family::Rust | Family::TsLike | Family::Python | Family::Go => "full",
        Family::CLike | Family::Generic => "nameHeuristic",
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Rust,
    TsLike,
    Python,
    Go,
    CLike,
    Generic,
}

fn family_for_ext(ext: &str) -> Family {
    match ext {
        "rs" => Family::Rust,
        "ts" | "mts" | "cts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Family::TsLike,
        "py" | "pyi" => Family::Python,
        "go" => Family::Go,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Family::CLike,
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

/// A multi-segment qualified path reference — a Rust `use` path or
/// fully-qualified call/type site (`scryer_extract::anchors::write_baseline`),
/// or a Go `pkg.Name` selector/type reference. The resolver maps the head
/// segment to a container (Rust: the crate manifest map; Go: the file's
/// import bindings) and the tail to a symbol, producing the cross-container
/// edges that bare-name resolution cannot. TS/JS and Python emit
/// [`ImportRef`] instead (their specs are module paths, not segment lists);
/// the generic-fallback languages emit neither yet.
#[derive(Debug, Clone)]
pub struct PathRef {
    /// Path segments head-to-leaf, e.g. `["scryer_extract", "anchors", "write_baseline"]`.
    pub segments: Vec<String>,
    /// 1-based line of the reference: the `use` line, or the call/type site.
    pub line: u32,
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
        Family::Go => collect_go(root, bytes, &mut defs),
        Family::CLike => collect_c(root, bytes, &mut defs),
        Family::Generic => collect_generic(root, bytes, &mut defs),
    }

    let mut idents: Vec<Ident> = Vec::new();
    collect_idents(root, bytes, &mut idents);

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
        _ => {}
    }

    let mut imports: Vec<ImportRef> = Vec::new();
    match family_for_ext(ext) {
        Family::TsLike => collect_ts_imports(root, bytes, &mut imports),
        Family::Python => collect_py_imports(root, bytes, &mut imports),
        Family::Go => collect_go_imports(root, bytes, &mut imports),
        _ => {}
    }

    let mut test_blocks: Vec<Def> = Vec::new();
    collect_string_named_calls(root, bytes, &mut test_blocks);

    Some(FileParse {
        defs,
        test_blocks,
        idents,
        paths,
        imports,
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

// --- Go ---

/// Top-level Go definitions. The generic fallback used to miss every struct
/// and interface (`type_spec` has the name, and nothing in the fallback's
/// kind set matches it) while minting bogus defs from interface method specs
/// (`method_elem` matched "method") — Go earns its own collector.
fn collect_go(node: Node, bytes: &[u8], defs: &mut Vec<Def>) {
    for child in named_children(node) {
        match child.kind() {
            "function_declaration" | "method_declaration" => {
                push_def(
                    defs,
                    field_text(child, "name", bytes),
                    child,
                    Vec::new(),
                    false,
                );
            }
            "type_declaration" => {
                for spec in named_children(child) {
                    if !matches!(spec.kind(), "type_spec" | "type_alias") {
                        continue;
                    }
                    // Span the SPEC, not the whole declaration: a grouped
                    // `type ( A …; B … )` block holds several defs.
                    let type_node = spec.child_by_field_name("type");
                    match type_node.map(|t| t.kind()) {
                        Some("struct_type") => {
                            let fields = go_struct_fields(type_node.unwrap(), bytes);
                            push_def(defs, field_text(spec, "name", bytes), spec, fields, true);
                        }
                        // An interface is a method set — behavioral, like a
                        // Rust trait, not a data shape.
                        _ => {
                            push_def(
                                defs,
                                field_text(spec, "name", bytes),
                                spec,
                                Vec::new(),
                                false,
                            );
                        }
                    }
                }
            }
            "const_declaration" | "var_declaration" => {
                for spec in named_children(child) {
                    if matches!(spec.kind(), "const_spec" | "var_spec") {
                        push_def(
                            defs,
                            field_text(spec, "name", bytes),
                            spec,
                            Vec::new(),
                            false,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Declared field names of a `struct_type`. A `field_declaration` may bind
/// several names to one type (`X, Y int`); embedded fields declare no
/// `field_identifier` and contribute none.
fn go_struct_fields(struct_type: Node, bytes: &[u8]) -> Vec<String> {
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
        for c in named_children(decl) {
            if c.kind() == "field_identifier" {
                if let Ok(text) = c.utf8_text(bytes) {
                    fields.push(text.to_string());
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
}
