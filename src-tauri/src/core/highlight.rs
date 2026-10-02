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

#[derive(Debug, serde::Serialize, Clone)]
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

/// Scryer's own Clojure form query, concatenated onto upstream's literal-only
/// one (the same layering the TypeScript config does over JavaScript).
///
/// The grammar is primitives-only: `(defn f [] …)` is a list whose head is an
/// ordinary symbol, so there is no keyword node for a query to capture and no
/// `name` field to find the defined symbol by. Both are matched on symbol TEXT
/// instead — which is also the only approach that can colour a reader-defined
/// form like `defroutes`.
///
/// Pattern order is load-bearing. `defrecord` matches both the general `^def`
/// rule and the type-defining one, so its name is captured twice — and
/// tree-sitter-highlight resolves that to the LATER pattern. The type rule
/// therefore comes last, or `User` would read as a function.
const CLOJURE_FORMS_QUERY: &str = r#"
;; Any other `def…` form — including user-defined defining macros
;; (`defroutes`, `defstate`, `defsc`), which is the whole point of matching on
;; text. The defined name is not anchored to sit immediately after the head, so
;; that `(def ^:private conn …)` still colours `conn` through its metadata; the
;; cost is that a bare `(def a b)` colours `b` too.
(list_lit
  .
  (sym_lit (sym_name) @keyword)
  (sym_lit (sym_name) @function)
  (#match? @keyword "^def"))

;; Type-defining forms name a type, not a function.
(list_lit
  .
  (sym_lit (sym_name) @keyword)
  (sym_lit (sym_name) @type)
  (#any-of? @keyword
    "defrecord" "deftype" "defprotocol" "definterface" "defstruct"))

;; Special forms, and the core macros that read as syntax.
(list_lit
  .
  (sym_lit (sym_name) @keyword)
  (#any-of? @keyword
    "if" "do" "let" "let*" "quote" "var" "fn" "fn*" "loop" "loop*" "recur"
    "throw" "try" "catch" "finally" "new" "set!" "monitor-enter" "monitor-exit"
    "ns" "in-ns" "require" "use" "import" "refer" "load" "comment"
    "letfn" "if-let" "if-some" "if-not" "when" "when-let" "when-some"
    "when-not" "when-first" "cond" "condp" "case" "for" "doseq" "dotimes"
    "while" "doto" "binding" "with-open" "with-local-vars" "with-redefs"
    "lazy-seq" "delay" "future" "locking" "assert"
    "->" "->>" "some->" "some->>" "as->" "cond->" "cond->>"
    "deftest" "testing" "is" "are"))
"#;

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
        "clj" | "cljs" | "cljc" | "cljr" => {
            let q = format!(
                "{}\n{}",
                tree_sitter_clojure::HIGHLIGHTS_QUERY,
                CLOJURE_FORMS_QUERY
            );
            build(tree_sitter_clojure::LANGUAGE.into(), "clojure", &q)
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The whole file highlights into one segment list per line — each line's
    /// segments concatenate back to the original text, with captures collapsed
    /// to coarse themeable classes.
    #[test]
    fn highlighting_returns_concatenable_segments_per_line() {
        let src = "fn main() {\n    let s = \"hi\";\n}\n";
        let lines = highlight_lines(Path::new("main.rs"), src).expect("rust is bundled");
        let originals: Vec<&str> = src.lines().collect();
        assert!(lines.len() >= originals.len());
        for (line, original) in lines.iter().zip(&originals) {
            let joined: String = line.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(&joined, original, "segments must concatenate back");
        }
        let kinds: std::collections::BTreeSet<&str> =
            lines.iter().flatten().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains("keyword"), "fn/let collapse to keyword: {kinds:?}");
        assert!(kinds.contains("string"), "the literal collapses to string: {kinds:?}");
    }

    /// Clojure has no keyword or definition NODE — `defn` is a plain symbol —
    /// so the form query matches on symbol text. A malformed query would make
    /// `build` return None silently and fall back to plain text, so this
    /// asserts the classes actually land.
    #[test]
    fn clojure_forms_highlight_by_symbol_text() {
        let src = "(ns app.core)\n\n(defn fetch-user\n  \"Doc.\"\n  [id]\n  (when (pos? id)\n    (inc id)))\n\n(defrecord User [id])\n\n(def ^:private conn nil)\n";
        let lines = highlight_lines(Path::new("core.clj"), src).expect("clojure is bundled");

        let originals: Vec<&str> = src.lines().collect();
        for (line, original) in lines.iter().zip(&originals) {
            let joined: String = line.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(&joined, original, "segments must concatenate back");
        }

        // Look up the class a given piece of text was given.
        let kind_of = |needle: &str| -> Vec<&str> {
            lines
                .iter()
                .flatten()
                .filter(|s| s.text == needle)
                .map(|s| s.kind.as_str())
                .collect()
        };
        assert_eq!(kind_of("defn"), vec!["keyword"], "a defining form reads as syntax");
        assert_eq!(kind_of("fetch-user"), vec!["function"], "the defined name");
        assert_eq!(kind_of("ns"), vec!["keyword"]);
        assert_eq!(kind_of("when"), vec!["keyword"], "a core macro reads as syntax");
        assert_eq!(
            kind_of("defrecord"),
            vec!["keyword"],
            "a type-defining form still reads as syntax"
        );
        assert_eq!(
            kind_of("User"),
            vec!["type"],
            "defrecord names a type, so its pattern must precede the ^def rule"
        );
        assert_eq!(
            kind_of("conn"),
            vec!["function"],
            "the name is found through its ^:private metadata"
        );
        assert_eq!(kind_of("\"Doc.\""), vec!["string"], "upstream's literal query still applies");
        // An ordinary call is not syntax: it stays in a default-class run
        // (segments coalesce, so `pos?` is not its own segment).
        assert!(
            lines
                .iter()
                .flatten()
                .any(|s| s.kind.is_empty() && s.text.contains("pos?")),
            "pos? is an ordinary call, left at the default colour"
        );
    }

    /// An unsupported language yields None so the caller renders plain text.
    #[test]
    fn unsupported_language_falls_back_to_plain() {
        assert!(highlight_lines(Path::new("notes.xyz"), "plain words\n").is_none());
        assert!(highlight_lines(Path::new("Makefile"), "all:\n").is_none(), "no extension");
    }
}
