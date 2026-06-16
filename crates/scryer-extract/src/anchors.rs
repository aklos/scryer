//! Git-free anchor fingerprints — the model's own memory of what its anchors
//! pointed at.
//!
//! At reconcile time, every sourceMap anchor is resolved (tree-sitter symbol
//! lookup when the anchor carries a symbol name, recorded line range otherwise)
//! and a content fingerprint of the resolved span is written to
//! `.scryer/.anchors.json`. The baseline is "what the model last saw" —
//! content-addressed, owned by scryer, independent of any VCS.
//!
//! At check time, only anchors in mtime-touched files are re-resolved:
//! - same content, new position → the anchor is **silently re-anchored**
//!   (sourceMap line ranges updated in place) — a moved function is not drift;
//! - different content → a `changed` observation (the claim's code changed);
//! - symbol gone from the file → `broken`; file gone → `fileMissing`.
//!
//! Observations are exactly that — observations. They are returned to the
//! caller (health report, drift check) and never written into the model;
//! statuses stay untouched.

use crate::lang;
use scryer_core::{drift, lock_model, read_model_at, write_model_at, ModelRef, ScryModel};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

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
fn span_hash(lines: &[&str], start: u32, end: u32) -> String {
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

fn fnv1a64_continue(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Per-call parse memo: each touched file is parsed at most once.
struct FileCache {
    files: HashMap<String, Option<(String, Option<lang::FileParse>)>>,
}

impl FileCache {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// Returns (source, parse) for a project-relative path; `None` when the
    /// file doesn't exist. Parse is `None` for unsupported grammars.
    fn get(&mut self, project: &Path, rel: &str) -> Option<&(String, Option<lang::FileParse>)> {
        if !self.files.contains_key(rel) {
            let loaded = std::fs::read_to_string(project.join(rel))
                .ok()
                .map(|source| {
                    let parse = lang::parse_file(Path::new(rel), &source);
                    (source, parse)
                });
            self.files.insert(rel.to_string(), loaded);
        }
        self.files.get(rel).and_then(|o| o.as_ref())
    }
}

/// Resolve one anchor against current file content: the span to fingerprint.
/// Symbol anchors resolve through the parse (nearest same-named def to `near`);
/// `None` means the symbol is gone. Anchors without a symbol use the recorded
/// line range, or the whole file.
fn resolve_span(
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
        for def in parse.defs.iter().filter(|d| d.name == name) {
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

/// Batch symbol-extent resolver — each touched file parses at most once.
/// Used to police whole-symbol responsibility mappings at write time.
pub struct ExtentResolver<'p> {
    project: &'p Path,
    cache: FileCache,
}

impl<'p> ExtentResolver<'p> {
    pub fn new(project: &'p Path) -> Self {
        Self {
            project,
            cache: FileCache::new(),
        }
    }

    /// Full (start, end) line extent of the named definition in `rel` —
    /// `None` when the file is missing, the grammar unsupported, or the
    /// symbol absent (those cases are the anchor checker's business, not a
    /// mapping-shape verdict).
    pub fn extent(&mut self, rel: &str, symbol: &str, near: Option<u32>) -> Option<(u32, u32)> {
        let (source, parse) = self.cache.get(self.project, rel)?;
        let parse = parse.as_ref()?;
        resolve_span(source, Some(parse), Some(symbol), near, None, None).ok()
    }
}

/// True when the explicit range `line..=end_line` covers the whole symbol
/// extent — with one line of tolerance each side, so starting under the
/// signature or stopping just shy of the closing brace still counts as a
/// whole-symbol mapping.
pub fn covers_extent(line: u32, end_line: u32, extent: (u32, u32)) -> bool {
    line <= extent.0 + 1 && end_line + 1 >= extent.1
}

/// Validation pass: responsibility mappings whose line range covers the whole
/// enclosing symbol. An explicit range is supposed to be a PROPER subset of
/// the symbol — "the whole definition" is encoded as a symbol-only anchor.
/// Schema declaration anchors (keyed by node id) legitimately span their
/// whole symbol and are skipped.
pub fn whole_symbol_warnings(model: &ScryModel, project: &Path) -> Vec<String> {
    let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut resolver = ExtentResolver::new(project);
    let mut out = Vec::new();
    let mut keys: Vec<&String> = model.source_map.keys().collect();
    keys.sort();
    for key in keys {
        if node_ids.contains(key.as_str()) {
            continue;
        }
        for loc in &model.source_map[key] {
            let (Some(sym), Some(line)) = (loc.symbol.as_deref(), loc.line) else {
                continue;
            };
            let end = loc.end_line.unwrap_or(line);
            let Some(extent) = resolver.extent(&loc.pattern, sym, Some(line)) else {
                continue;
            };
            if covers_extent(line, end, extent) {
                out.push(format!(
                    "{}: {} L{}-{} covers the whole symbol `{}` (L{}-{}) — map the specific lines that do the work, or drop the range (a symbol-only anchor means the whole definition)",
                    key, loc.pattern, line, end, sym, extent.0, extent.1
                ));
            }
        }
    }
    out
}

/// Resolve and fingerprint every sourceMap anchor against the working tree as
/// it stands, and write the baseline. Call at every reconcile point (build
/// completion, drift-check completion, `reconcile_drift`, sync seeding).
/// Anchors whose file is missing are skipped — there is nothing to remember.
pub fn write_baseline(r: &ModelRef) -> Result<usize, String> {
    let model = read_model_at(r)?;
    let project = r.project_path();
    let mut cache = FileCache::new();
    let mut anchors: Vec<AnchorEntry> = Vec::new();

    let mut keys: Vec<&String> = model.source_map.keys().collect();
    keys.sort();
    for key in keys {
        for loc in &model.source_map[key] {
            let Some((source, parse)) = cache.get(project, &loc.pattern) else {
                continue;
            };
            let lines: Vec<&str> = source.lines().collect();
            let Ok((start, end)) = resolve_span(
                source,
                parse.as_ref(),
                loc.symbol.as_deref(),
                loc.line,
                loc.line,
                loc.end_line,
            ) else {
                continue; // symbol unresolvable right now — nothing to remember
            };
            anchors.push(AnchorEntry {
                key: key.clone(),
                file: loc.pattern.clone(),
                symbol: loc.symbol.clone(),
                start,
                end,
                hash: span_hash(&lines, start, end),
            });
        }
    }

    let count = anchors.len();
    let json = serde_json::to_string(&AnchorBaseline { anchors }).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(r.dir()).map_err(|e| e.to_string())?;
    std::fs::write(r.anchors_path(), json).map_err(|e| e.to_string())?;
    Ok(count)
}

fn read_baseline(r: &ModelRef) -> Option<AnchorBaseline> {
    let raw = std::fs::read_to_string(r.anchors_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Check every fingerprinted anchor whose file was touched since the sync
/// anchor. Moved-but-unchanged symbols are re-anchored in place (sourceMap line
/// ranges + baseline updated, model written under the lock); content changes
/// and broken anchors come back as observations. No baseline → empty (the
/// first reconcile seeds it).
pub fn check_anchors(r: &ModelRef) -> Result<AnchorCheck, String> {
    let Some(mut baseline) = read_baseline(r) else {
        return Ok(AnchorCheck::default());
    };
    let project = r.project_path().to_path_buf();

    let _lock = lock_model(r)?;
    let mut model = read_model_at(r)?;
    let sync = scryer_core::read_sync_state(r);
    let touched: BTreeSet<String> = drift::changed_files_since(&project, &sync);

    // The mtime walk only sees files that exist — a deleted anchor file never
    // lands in `touched`. Existence is a cheap stat per distinct baseline file,
    // so missing files are swept unconditionally.
    let mut exists: HashMap<String, bool> = HashMap::new();
    for entry in &baseline.anchors {
        if !exists.contains_key(&entry.file) {
            let e = project.join(&entry.file).exists();
            exists.insert(entry.file.clone(), e);
        }
    }
    let any_missing = exists.values().any(|e| !e);
    if touched.is_empty() && !any_missing {
        return Ok(AnchorCheck::default());
    }

    // sourceMap keys → hosts, for naming observations.
    let mut host_of: HashMap<&str, (&str, &str)> = HashMap::new();
    for node in &model.nodes {
        host_of.insert(node.id.as_str(), (node.id.as_str(), node.name.as_str()));
        for resp in &node.responsibilities {
            host_of.insert(resp.id.as_str(), (node.id.as_str(), node.name.as_str()));
        }
    }
    for group in &model.groups {
        for resp in &group.responsibilities {
            host_of.insert(resp.id.as_str(), (group.id.as_str(), group.name.as_str()));
        }
    }
    let host_of: HashMap<String, (String, String)> = host_of
        .into_iter()
        .map(|(k, (a, b))| (k.to_string(), (a.to_string(), b.to_string())))
        .collect();

    let mut cache = FileCache::new();
    let mut out: Vec<AnchorObservation> = Vec::new();
    let mut reanchored = 0usize;
    let mut model_dirty = false;
    let mut baseline_dirty = false;

    for entry in baseline.anchors.iter_mut() {
        let file_exists = exists.get(&entry.file).copied().unwrap_or(true);
        if file_exists && !touched.contains(&entry.file) {
            continue;
        }
        // The anchor may have been edited/removed since the baseline — only
        // check entries the model still carries (matched by file + symbol).
        let still_anchored = model.source_map.get(&entry.key).is_some_and(|locs| {
            locs.iter()
                .any(|l| l.pattern == entry.file && l.symbol == entry.symbol)
        });
        if !still_anchored {
            continue;
        }
        let Some((host_id, host_name)) = host_of.get(&entry.key).cloned() else {
            continue; // dangling sourceMap key — validate_model's department
        };

        let observe = |state: AnchorState, entry: &AnchorEntry| AnchorObservation {
            key: entry.key.clone(),
            host_id: host_id.clone(),
            host_name: host_name.clone(),
            file: entry.file.clone(),
            symbol: entry.symbol.clone(),
            state,
        };

        let Some((source, parse)) = cache.get(&project, &entry.file) else {
            out.push(observe(AnchorState::FileMissing, entry));
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();

        let resolved = resolve_span(
            source,
            parse.as_ref(),
            entry.symbol.as_deref(),
            Some(entry.start),
            Some(entry.start),
            Some(entry.end),
        );
        let Ok((start, end)) = resolved else {
            out.push(observe(AnchorState::Broken, entry));
            continue;
        };
        let hash = span_hash(&lines, start, end);

        if hash != entry.hash {
            out.push(observe(AnchorState::Changed, entry));
            continue;
        }
        if (start, end) != (entry.start, entry.end) {
            // Same content, new position: re-anchor silently. Update every
            // matching sourceMap location that recorded line numbers, and the
            // baseline span, so the lens stays sharp without flagging anything.
            if let Some(locs) = model.source_map.get_mut(&entry.key) {
                for l in locs.iter_mut() {
                    if l.pattern == entry.file && l.symbol == entry.symbol && l.line.is_some() {
                        l.line = Some(start);
                        l.end_line = Some(end);
                        model_dirty = true;
                    }
                }
            }
            entry.start = start;
            entry.end = end;
            baseline_dirty = true;
            reanchored += 1;
        }
    }

    if model_dirty {
        write_model_at(r, &model)?;
    }
    if baseline_dirty {
        let json = serde_json::to_string(&baseline).map_err(|e| e.to_string())?;
        let _ = std::fs::write(r.anchors_path(), json);
    }

    out.sort_by(|a, b| (&a.host_id, &a.key, &a.file).cmp(&(&b.host_id, &b.key, &b.file)));
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.state == b.state);
    Ok(AnchorCheck {
        observations: out,
        reanchored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{
        drift::SyncState, Kind, Node, Responsibility, ScryModel, SourceLocation, Status,
    };

    fn project_with(file: &str, content: &str) -> (tempfile::TempDir, ModelRef) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join(file), content).unwrap();
        let r = ModelRef::ProjectLocal(root.to_path_buf());
        (dir, r)
    }

    #[test]
    fn covers_extent_tolerates_one_line_each_side() {
        let extent = (10, 40);
        assert!(covers_extent(10, 40, extent)); // exact
        assert!(covers_extent(11, 39, extent)); // body minus signature/brace
        assert!(covers_extent(5, 50, extent)); // over-covering
        assert!(!covers_extent(12, 40, extent)); // proper subset — keep
        assert!(!covers_extent(10, 25, extent)); // proper subset — keep
    }

    #[test]
    fn extent_resolver_finds_symbol_extent() {
        let src = "fn other() {}\n\nfn target() {\n    let a = 1;\n    let b = 2;\n}\n";
        let (dir, r) = project_with("src/main.rs", src);
        let mut resolver = ExtentResolver::new(r.project_path());
        assert_eq!(resolver.extent("src/main.rs", "target", None), Some((3, 6)));
        assert_eq!(resolver.extent("src/main.rs", "missing", None), None);
        drop(dir);
    }

    fn leaf_model(anchor_symbol: &str, file: &str, line: u32, end: u32) -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(Node {
            id: "sym".into(),
            kind: Kind::Symbol,
            name: anchor_symbol.into(),
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                id: "r1".into(),
                statement: "does the thing".into(),
                status: Some(Status::Implemented),
                vagrant: None,
                stale: None,
                locked: None,
                relocated_to: None,
                relocated_from: None,
                directives: Vec::new(),
                last_touched_at: None,
                changed_from: None,
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            relocated: None,
            locked: None,
            relocated_to: None,
            relocated_from: None,
        });
        m.source_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: file.into(),
                symbol: Some(anchor_symbol.into()),
                line: Some(line),
                end_line: Some(end),
                command: None,
            }],
        );
        m
    }

    const TS: &str = "export function alpha() {\n    return 1;\n}\n\nexport function beta() {\n    return 1;\n}\n";

    /// Reconcile the working tree as it stands: sync anchor now + baseline.
    fn reconcile(r: &ModelRef) {
        scryer_core::write_sync_state(
            r,
            &SyncState {
                reconciled_at: drift::now_secs(),
                commit: None,
                ..Default::default()
            },
        )
        .unwrap();
        write_baseline(r).unwrap();
    }

    fn touch_gate() {
        // mtime granularity — edits must land in a newer second than the anchor.
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }

    /// An edit elsewhere in the file is not drift for this anchor; an edit
    /// inside the anchored symbol is.
    #[test]
    fn changed_only_when_the_anchored_span_changes() {
        let (_dir, r) = project_with("src/m.ts", TS);
        scryer_core::write_model_at(&r, &leaf_model("alpha", "src/m.ts", 1, 3)).unwrap();
        reconcile(&r);

        // Edit beta only.
        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.ts"),
            TS.replace(
                "function beta() {\n    return 1;",
                "function beta() {\n    return 2;",
            ),
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);

        // Edit alpha.
        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.ts"),
            TS.replace(
                "function alpha() {\n    return 1;",
                "function alpha() {\n    return 42;",
            ),
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::Changed);
        assert_eq!(check.observations[0].key, "r1");
        assert_eq!(check.observations[0].host_id, "sym");
    }

    /// A symbol that moves without changing is re-anchored silently: the
    /// sourceMap line range updates, no observation fires.
    #[test]
    fn moved_symbol_is_reanchored_not_drift() {
        let (_dir, r) = project_with("src/m.ts", TS);
        scryer_core::write_model_at(&r, &leaf_model("alpha", "src/m.ts", 1, 3)).unwrap();
        reconcile(&r);

        touch_gate();
        // Push alpha down by three comment lines; its text is unchanged.
        std::fs::write(
            r.project_path().join("src/m.ts"),
            format!("// pad\n// pad\n// pad\n{TS}"),
        )
        .unwrap();

        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);
        assert_eq!(check.reanchored, 1);
        let m = read_model_at(&r).unwrap();
        let loc = &m.source_map["r1"][0];
        assert_eq!(loc.line, Some(4), "anchor followed the symbol");
        assert_eq!(loc.end_line, Some(6));

        // And the healed anchor stays quiet on the next check.
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty());
        assert_eq!(check.reanchored, 0);
    }

    /// A symbol deleted from the file is a broken anchor; a deleted file is
    /// fileMissing. No git anywhere.
    #[test]
    fn broken_and_missing_anchors_surface() {
        let (_dir, r) = project_with("src/m.ts", TS);
        scryer_core::write_model_at(&r, &leaf_model("alpha", "src/m.ts", 1, 3)).unwrap();
        reconcile(&r);

        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.ts"),
            "export function beta() {\n    return 1;\n}\n",
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::Broken);

        // Restore, reconcile, then delete the file outright. Deletion never
        // shows in the mtime walk — the existence sweep catches it anyway.
        std::fs::write(r.project_path().join("src/m.ts"), TS).unwrap();
        reconcile(&r);
        std::fs::remove_file(r.project_path().join("src/m.ts")).unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::FileMissing);
    }
}
