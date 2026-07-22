//! In-process symbol resolution for the code inspector.
//!
//! Given a file + an identifier the agent recorded (`SourceLocation.symbol`),
//! resolve the definition's line range by parsing with tree-sitter. Grammars
//! are compiled into the binary, so this needs no external tool. Languages
//! without a bundled grammar fall back to the caller's text search.

use std::path::Path;
use tree_sitter::{Language, Parser};

/// Map a file extension to a bundled tree-sitter grammar, if one exists.
fn language_for_ext(ext: &str) -> Option<Language> {
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

/// True if a node kind names a definition we'd want to surface as the symbol's
/// body (rather than, say, a call or a parameter that happens to share a name).
fn is_definition_kind(kind: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "function", "method", "class", "struct", "enum", "interface", "trait",
        "impl", "module", "declaration", "definition", "declarator", "type",
        "constructor", "object", "property", "field", "const", "macro",
    ];
    NEEDLES.iter().any(|n| kind.contains(n))
}

/// Resolve `symbol` to a 1-based inclusive line range in `source`. Walks the
/// parse tree for nodes whose `name` field equals the symbol, preferring
/// definition-shaped kinds and, among those, the one nearest `line_hint`.
/// Returns `None` for unsupported languages or when the symbol isn't found.
pub fn resolve(
    path: &Path,
    source: &str,
    symbol: &str,
    line_hint: Option<u32>,
) -> Option<(u32, u32)> {
    let ext = path.extension()?.to_str()?;
    let language = language_for_ext(ext)?;

    let mut parser = Parser::new();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();

    // (start, end, is_def) of the best match so far.
    let mut best: Option<(u32, u32, bool)> = None;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let mut consider = |node: tree_sitter::Node, is_def: bool| {
            let start = node.start_position().row as u32 + 1;
            let end = node.end_position().row as u32 + 1;
            let better = match best {
                None => true,
                Some((bs, _, bdef)) => {
                    if is_def != bdef {
                        is_def // a definition beats a non-definition
                    } else {
                        match line_hint {
                            Some(h) => {
                                (start as i64 - h as i64).abs() < (bs as i64 - h as i64).abs()
                            }
                            None => false,
                        }
                    }
                }
            };
            if better {
                best = Some((start, end, is_def));
            }
        };
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.utf8_text(bytes).ok() == Some(symbol) {
                consider(node, is_definition_kind(node.kind()));
            }
        }
        // A test anchored by its NAME: `it("…")` / `test("…")` / `t.Run("…")` —
        // a call whose first argument is the string the anchor recorded. The
        // whole call (callback body included) is the definition-shaped span.
        // Identifier defs outrank it, so a name that is both stays a symbol.
        if node.kind().contains("call") && first_string_arg(node, bytes).as_deref() == Some(symbol) {
            consider(node, false);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    best.map(|(s, e, _)| (s, e))
}

/// The literal content of a call's first argument when it is a plain string:
/// argument list via the `arguments` field (or the named child whose kind says
/// "argument"), quotes trimmed. Interpolated templates yield `None` — a
/// computed name can't be matched.
fn first_string_arg(call: tree_sitter::Node, bytes: &[u8]) -> Option<String> {
    let args = call.child_by_field_name("arguments").or_else(|| {
        let mut cur = call.walk();
        let found = call.named_children(&mut cur).find(|n| n.kind().contains("argument"));
        found
    })?;
    let mut cur = args.walk();
    let first = args.named_children(&mut cur).next()?;
    if !first.kind().contains("string") {
        return None;
    }
    let raw = first.utf8_text(bytes).ok()?;
    let inner = raw
        .strip_prefix(['"', '\'', '`'])
        .and_then(|s| s.strip_suffix(['"', '\'', '`']))?;
    (!inner.is_empty() && !inner.contains("${")).then(|| inner.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ts_function() {
        let src = "import x;\n\nexport function autoLayout(a) {\n  return a + 1;\n}\n";
        assert_eq!(resolve(Path::new("f.ts"), src, "autoLayout", None), Some((3, 5)));
    }

    #[test]
    fn tsx_const_arrow() {
        let src = "export const useZoom = () => {\n  return 1;\n};\n";
        assert_eq!(resolve(Path::new("f.tsx"), src, "useZoom", None), Some((1, 3)));
    }

    #[test]
    fn rust_fn_skips_decoy() {
        let src = "fn helper() {}\n\npub fn target(x: u32) -> u32 {\n    x + 1\n}\n";
        assert_eq!(resolve(Path::new("f.rs"), src, "target", None), Some((3, 5)));
    }

    #[test]
    fn python_def() {
        let src = "import os\n\ndef target(a):\n    return a\n";
        assert_eq!(resolve(Path::new("f.py"), src, "target", None), Some((3, 4)));
    }

    #[test]
    fn unsupported_ext_is_none() {
        assert_eq!(resolve(Path::new("f.zzz"), "x", "x", None), None);
    }

    #[test]
    fn ts_test_block_by_name_string() {
        let src = "describe(\"webhook verify\", () => {\n  it(\"rejects an unsigned webhook\", async () => {\n    expect(res.status).toBe(403);\n  });\n});\n";
        assert_eq!(
            resolve(Path::new("f.spec.ts"), src, "rejects an unsigned webhook", None),
            Some((2, 4)),
        );
        // The enclosing describe resolves by ITS name, spanning the suite.
        assert_eq!(
            resolve(Path::new("f.spec.ts"), src, "webhook verify", None),
            Some((1, 5)),
        );
    }

    #[test]
    fn identifier_def_outranks_same_named_string_call() {
        let src = "function login() {\n  return 1;\n}\nit(\"login\", () => login());\n";
        assert_eq!(resolve(Path::new("f.ts"), src, "login", None), Some((1, 3)));
    }
}
