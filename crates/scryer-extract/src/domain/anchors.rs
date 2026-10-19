//! How an anchor's span is recognised and fingerprinted: the pure resolution
//! of a symbol (or a line range, or a whole file) to its span in a given
//! source text, and the content hash a verdict records for it.

use crate::domain::lang;

/// A sourceMap pattern with glob metacharacters claims territory, not a file.
pub(crate) fn is_glob_pattern(p: &str) -> bool {
    p.contains(['*', '?', '['])
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

