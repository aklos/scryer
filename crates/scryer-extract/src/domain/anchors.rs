//! What an anchor IS and how a span is recognised: the fingerprint entries the
//! baseline stores, the observations a check produces, and the pure resolution
//! of a symbol to its span in a given source text. Reading files and the model
//! is the infrastructure's job — see `infrastructure::anchors`.

use crate::domain::lang;
use scryer_core::ScryModel;
use serde::{Deserialize, Serialize};

/// One fingerprinted anchor: a sourceMap location resolved to a concrete span
/// at reconcile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorEntry {
    /// The sourceMap key — a responsibility id, or a node id for a data-shape
    /// declaration anchor.
    pub key: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// 1-based inclusive line span that was fingerprinted.
    pub start: u32,
    pub end: u32,
    /// FNV-1a 64 hex of the span text (line endings normalized).
    pub hash: String,
    /// How many same-named defs the file held at baseline time (symbol anchors
    /// only; 0 = unknown or not a symbol anchor). Lets the checker tell "my
    /// def was deleted while a sibling survives" (count shrank, no content
    /// match → broken) from "my def was edited in place" (changed).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub peers: u32,
    /// The originating sourceMap GLOB when this entry came from expanding one
    /// (`file` is then a concrete matched file). Links the entry back to its
    /// model location, which spells the glob, not the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl AnchorEntry {
    /// What the model's sourceMap location spells for this entry: the glob it
    /// was expanded from, or the literal file path.
    pub(crate) fn source_pattern(&self) -> &str {
        self.pattern.as_deref().unwrap_or(&self.file)
    }
}

pub(crate) fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The model locations behind a baseline key: `test:`-namespaced keys read
/// the test_map (claim → attached test), plain keys the source_map. One
/// baseline fingerprints both dimensions.
pub(crate) fn keyed_locs<'m>(
    model: &'m ScryModel,
    key: &str,
) -> Option<&'m Vec<scryer_core::SourceLocation>> {
    match scryer_core::test_resp_id(key) {
        Some(id) => model.test_map.get(id),
        None => model.source_map.get(key),
    }
}

pub(crate) fn keyed_locs_mut<'m>(
    model: &'m mut ScryModel,
    key: &str,
) -> Option<&'m mut Vec<scryer_core::SourceLocation>> {
    match scryer_core::test_resp_id(key) {
        Some(id) => model.test_map.get_mut(id),
        None => model.source_map.get_mut(key),
    }
}

/// A sourceMap pattern with glob metacharacters claims territory, not a file.
pub(crate) fn is_glob_pattern(p: &str) -> bool {
    p.contains(['*', '?', '['])
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorBaseline {
    #[serde(default)]
    pub anchors: Vec<AnchorEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AnchorState {
    /// The anchored span's content changed since the reconcile.
    Changed,
    /// The anchor's symbol no longer exists in the file.
    Broken,
    /// The anchor's file no longer exists.
    FileMissing,
}

/// A blur observation: one anchor whose code no longer matches what the model
/// last saw. Scoping for a semantic re-check, never a verdict.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorObservation {
    pub key: String,
    pub host_id: String,
    pub host_name: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub state: AnchorState,
}

/// Outcome of an anchor check.
#[derive(Debug, Default)]
pub struct AnchorCheck {
    pub observations: Vec<AnchorObservation>,
    /// Anchors whose symbol moved without changing — sourceMap line ranges
    /// were updated in place (self-healing, not drift).
    pub reanchored: usize,
}

/// Hash a 1-based inclusive line span with FNV-1a 64 — deterministic and
/// dependency-free (std's DefaultHasher is documented unstable across
/// releases). Line endings are normalized so a CRLF/LF round-trip never reads
/// as a content change.
pub(crate) fn span_hash(lines: &[&str], start: u32, end: u32) -> String {
    let s = start.max(1) as usize - 1;
    let e = (end as usize).min(lines.len());
    let mut h: u64 = 0xcbf29ce484222325;
    for line in lines.iter().take(e).skip(s) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        h = fnv1a64_continue(h, line.as_bytes());
        h = fnv1a64_continue(h, b"\n");
    }
    format!("{h:016x}")
}

pub(crate) fn fnv1a64_continue(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 1-based start lines of every `len`-line window whose content hashes to
/// `hash`, up to `limit` (the caller treats more than one as ambiguous, so
/// scanning further is wasted work). This is what lets a LINE-ONLY anchor
/// survive an insertion above it: the remembered content is searched for,
/// not just re-read at the remembered position.
pub(crate) fn find_spans_by_hash(lines: &[&str], len: u32, hash: &str, limit: usize) -> Vec<u32> {
    let mut out = Vec::new();
    let n = lines.len() as u32;
    if len == 0 || n < len {
        return out;
    }
    for start in 1..=(n - len + 1) {
        if span_hash(lines, start, start + len - 1) == hash {
            out.push(start);
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

/// Every definition matching `name` — identifier defs first, then string-named
/// test blocks (`it("…")`), so a test anchored by its name resolves through the
/// same lookup as a code symbol while identifier defs keep priority on ties.
pub(crate) fn named_defs<'p>(parse: &'p lang::FileParse, name: &'p str) -> impl Iterator<Item = &'p lang::Def> {
    parse
        .defs
        .iter()
        .chain(parse.test_blocks.iter())
        .filter(move |d| d.name == name)
}

/// Resolve one anchor against current file content: the span to fingerprint.
/// Symbol anchors resolve through the parse (nearest same-named def to `near`);
/// `None` means the symbol is gone. Anchors without a symbol use the recorded
/// line range, or the whole file.
pub(crate) fn resolve_span(
    source: &str,
    parse: Option<&lang::FileParse>,
    symbol: Option<&str>,
    near: Option<u32>,
    line: Option<u32>,
    end_line: Option<u32>,
) -> Result<(u32, u32), ()> {
    let line_count = source.lines().count().max(1) as u32;
    if let (Some(name), Some(parse)) = (symbol, parse) {
        let mut best: Option<&lang::Def> = None;
        // Identifier defs first, then string-named test blocks (`it("…")`) —
        // an attached test anchored by its name resolves like any symbol.
        for def in named_defs(parse, name) {
            best = match best {
                None => Some(def),
                Some(cur) => {
                    let anchor = near.unwrap_or(cur.start_line);
                    let d = |x: u32| x.abs_diff(anchor);
                    if d(def.start_line) < d(cur.start_line) {
                        Some(def)
                    } else {
                        Some(cur)
                    }
                }
            };
        }
        return match best {
            Some(def) => Ok((def.start_line, def.end_line.max(def.start_line))),
            None => Err(()), // symbol gone — broken anchor
        };
    }
    // Symbol anchors in unparseable files degrade to their recorded range.
    match line {
        Some(l) => Ok((l, end_line.unwrap_or(l).max(l).min(line_count.max(l)))),
        None => Ok((1, line_count)),
    }
}

/// True when the explicit range `line..=end_line` covers the whole symbol
/// extent — with one line of tolerance each side, so starting under the
/// signature or stopping just shy of the closing brace still counts as a
/// whole-symbol mapping.
pub fn covers_extent(line: u32, end_line: u32, extent: (u32, u32)) -> bool {
    line <= extent.0 + 1 && end_line + 1 >= extent.1
}

/// A sourceMap anchor with NO fingerprint in the baseline: a span the
/// tripwire cannot watch. No drift will ever fire for it, so health must say
/// so — a silent anchor reading as green is the trust-burning failure mode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UntrackedAnchor {
    pub key: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}
