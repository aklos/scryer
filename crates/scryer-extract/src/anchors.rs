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
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    fn source_pattern(&self) -> &str {
        self.pattern.as_deref().unwrap_or(&self.file)
    }
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The model locations behind a baseline key: `test:`-namespaced keys read
/// the test_map (claim → attached test), plain keys the source_map. One
/// baseline fingerprints both dimensions.
fn keyed_locs<'m>(
    model: &'m ScryModel,
    key: &str,
) -> Option<&'m Vec<scryer_core::SourceLocation>> {
    match scryer_core::test_resp_id(key) {
        Some(id) => model.test_map.get(id),
        None => model.source_map.get(key),
    }
}

fn keyed_locs_mut<'m>(
    model: &'m mut ScryModel,
    key: &str,
) -> Option<&'m mut Vec<scryer_core::SourceLocation>> {
    match scryer_core::test_resp_id(key) {
        Some(id) => model.test_map.get_mut(id),
        None => model.source_map.get_mut(key),
    }
}

/// A sourceMap pattern with glob metacharacters claims territory, not a file.
fn is_glob_pattern(p: &str) -> bool {
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

/// 1-based start lines of every `len`-line window whose content hashes to
/// `hash`, up to `limit` (the caller treats more than one as ambiguous, so
/// scanning further is wasted work). This is what lets a LINE-ONLY anchor
/// survive an insertion above it: the remembered content is searched for,
/// not just re-read at the remembered position.
fn find_spans_by_hash(lines: &[&str], len: u32, hash: &str, limit: usize) -> Vec<u32> {
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

/// Every definition matching `name` — identifier defs first, then string-named
/// test blocks (`it("…")`), so a test anchored by its name resolves through the
/// same lookup as a code symbol while identifier defs keep priority on ties.
fn named_defs<'p>(parse: &'p lang::FileParse, name: &'p str) -> impl Iterator<Item = &'p lang::Def> {
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
/// Glob patterns expand to one entry per matched file (whole-file span, or
/// the symbol's span where one is named) — they used to fall out of the
/// baseline silently. Anchors whose file is missing are skipped — there is
/// nothing to remember.
pub fn write_baseline(r: &ModelRef) -> Result<usize, String> {
    let model = read_model_at(r)?;
    let project = r.project_path();
    let mut cache = FileCache::new();
    let mut anchors: Vec<AnchorEntry> = Vec::new();
    // Walked lazily — only when a glob anchor exists.
    let mut project_files: Option<std::collections::BTreeSet<String>> = None;

    // Both anchor dimensions share the baseline: source anchors under their
    // bare key, test anchors (claim → attached test) under `test:{id}`.
    let mut keyed: Vec<(String, &Vec<scryer_core::SourceLocation>)> = model
        .source_map
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .chain(model.test_map.iter().map(|(k, v)| (scryer_core::test_key(k), v)))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, locs) in keyed {
        for loc in locs {
            let mut fingerprint = |file: &str, from_glob: Option<&str>| {
                let Some((source, parse)) = cache.get(project, file) else {
                    return;
                };
                let lines: Vec<&str> = source.lines().collect();
                // Glob expansions ignore the loc's line range (it describes no
                // single file); literal anchors keep it.
                let (line, end_line) = if from_glob.is_some() {
                    (None, None)
                } else {
                    (loc.line, loc.end_line)
                };
                let Ok((start, end)) = resolve_span(
                    source,
                    parse.as_ref(),
                    loc.symbol.as_deref(),
                    line,
                    line,
                    end_line,
                ) else {
                    return; // symbol unresolvable right now — nothing to remember
                };
                let peers = match (&loc.symbol, parse) {
                    (Some(name), Some(p)) => named_defs(p, name).count() as u32,
                    _ => 0,
                };
                anchors.push(AnchorEntry {
                    key: key.clone(),
                    file: file.to_string(),
                    symbol: loc.symbol.clone(),
                    start,
                    end,
                    hash: span_hash(&lines, start, end),
                    peers,
                    pattern: from_glob.map(|g| g.to_string()),
                });
            };
            if is_glob_pattern(&loc.pattern) {
                let Ok(pattern) = glob::Pattern::new(&loc.pattern) else {
                    continue;
                };
                let files = project_files
                    .get_or_insert_with(|| crate::list_project_files(project));
                for file in files.iter().filter(|f| pattern.matches(f)) {
                    fingerprint(file, Some(&loc.pattern));
                }
            } else {
                fingerprint(&loc.pattern, None);
            }
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

/// Fate of one checked anchor: computed against current content (borrowing
/// the parse cache), then applied to the model and baseline.
enum Fate {
    /// Content still at the remembered span.
    Quiet,
    /// Same content at a new span — re-anchor silently.
    Move(u32, u32),
    Changed,
    Broken,
}

/// Search other project files (same extension) for a missing file's anchor
/// content. Returns the unique `(file, start, end)` hosting the remembered
/// span — `None` when nothing, or more than one place, matches: a rescue is a
/// silent sourceMap rewrite, so it must never guess. Content-exact only; a
/// file renamed AND edited in one step stays `fileMissing`.
fn rescue_missing(
    entry: &AnchorEntry,
    project: &Path,
    candidates: &[String],
    cache: &mut FileCache,
) -> Option<(String, u32, u32)> {
    let ext = Path::new(&entry.file).extension().and_then(|e| e.to_str());
    let len = entry.end.saturating_sub(entry.start) + 1;
    let mut found: Option<(String, u32, u32)> = None;
    for cand in candidates {
        if *cand == entry.file || Path::new(cand).extension().and_then(|e| e.to_str()) != ext {
            continue;
        }
        let Some((source, parse)) = cache.get(project, cand) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        let spans: Vec<(u32, u32)> = match (&entry.symbol, parse) {
            (Some(name), Some(p)) => named_defs(p, name)
                .map(|d| (d.start_line, d.end_line.max(d.start_line)))
                .filter(|&(s, e)| span_hash(&lines, s, e) == entry.hash)
                .collect(),
            (Some(_), None) => Vec::new(), // a symbol anchor needs a parse
            (None, _) => find_spans_by_hash(&lines, len, &entry.hash, 2)
                .into_iter()
                .map(|s| (s, s + len - 1))
                .collect(),
        };
        for (s, e) in spans {
            if found.is_some() {
                return None; // ambiguous, within or across files
            }
            found = Some((cand.clone(), s, e));
        }
    }
    found
}

/// Check every fingerprinted anchor whose file was touched since the sync
/// anchor. Moved-but-unchanged symbols are re-anchored in place (sourceMap line
/// ranges + baseline updated, model written under the lock); content changes
/// and broken anchors come back as observations. A missing file gets one
/// content-hash rescue attempt (rename tracking) before it reports. No
/// baseline → empty (the first reconcile seeds it).
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
    // Rescue candidates for missing files — the project walk runs at most
    // once per check, and only when a file actually disappeared.
    let mut rescue_candidates: Option<Vec<String>> = None;

    for entry in baseline.anchors.iter_mut() {
        let file_exists = exists.get(&entry.file).copied().unwrap_or(true);
        if file_exists && !touched.contains(&entry.file) {
            continue;
        }
        // The anchor may have been edited/removed since the baseline — only
        // check entries the model still carries (matched by the loc's spelled
        // pattern — the glob for expanded entries — plus symbol).
        let still_anchored = keyed_locs(&model, &entry.key).is_some_and(|locs| {
            locs.iter()
                .any(|l| l.pattern == entry.source_pattern() && l.symbol == entry.symbol)
        });
        if !still_anchored {
            continue;
        }
        // A test anchor's host is the claim the test backs.
        let host_key = scryer_core::test_resp_id(&entry.key).unwrap_or(&entry.key);
        let Some((host_id, host_name)) = host_of.get(host_key).cloned() else {
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

        if !file_exists {
            // The mtime walk can't see a rename (`mv` preserves mtimes) and a
            // deleted file has nothing to re-read — but the baseline remembers
            // the exact content. Search same-extension project files for it:
            // exactly one match is a rename to follow, anything else stays
            // missing (never guess). A glob-expanded entry searches only
            // within its own glob's territory, and never rewrites the model —
            // the loc still spells the glob, which covers the new path.
            let candidates = rescue_candidates.get_or_insert_with(|| {
                crate::list_project_files(&project).into_iter().collect()
            });
            let rescued = match &entry.pattern {
                Some(source_glob) => glob::Pattern::new(source_glob).ok().and_then(|p| {
                    let scoped: Vec<String> = candidates
                        .iter()
                        .filter(|c| p.matches(c))
                        .cloned()
                        .collect();
                    rescue_missing(entry, &project, &scoped, &mut cache)
                }),
                None => rescue_missing(entry, &project, candidates, &mut cache),
            };
            match rescued {
                Some((new_file, start, end)) => {
                    if entry.pattern.is_none() {
                        if let Some(locs) = keyed_locs_mut(&mut model, &entry.key) {
                            for l in locs.iter_mut() {
                                if l.pattern == entry.file && l.symbol == entry.symbol {
                                    l.pattern = new_file.clone();
                                    if l.line.is_some() {
                                        l.line = Some(start);
                                        l.end_line = Some(end);
                                    }
                                    model_dirty = true;
                                }
                            }
                        }
                    }
                    entry.file = new_file;
                    entry.start = start;
                    entry.end = end;
                    baseline_dirty = true;
                    reanchored += 1;
                }
                None => out.push(observe(AnchorState::FileMissing, entry)),
            }
            continue;
        }
        let Some((source, parse)) = cache.get(&project, &entry.file) else {
            out.push(observe(AnchorState::FileMissing, entry)); // unreadable
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();

        let fate = if let (Some(name), Some(parse)) = (entry.symbol.as_deref(), parse.as_ref()) {
            let defs: Vec<&lang::Def> = named_defs(parse, name).collect();
            if defs.is_empty() {
                Fate::Broken
            } else {
                // Hash-first: the def carrying the REMEMBERED content is the
                // anchored one, even when a same-named sibling sits nearer to
                // the old position (Rust impl methods share one flat
                // namespace, so nearest-wins used to adopt the wrong def).
                let matched: Option<(u32, u32)> = defs
                    .iter()
                    .map(|d| (d.start_line, d.end_line.max(d.start_line)))
                    .filter(|&(s, e)| span_hash(&lines, s, e) == entry.hash)
                    .min_by_key(|(s, _)| s.abs_diff(entry.start));
                match matched {
                    Some((s, e)) if (s, e) == (entry.start, entry.end) => Fate::Quiet,
                    Some((s, e)) => Fate::Move(s, e),
                    // No def carries the remembered content AND the same-name
                    // population shrank: the anchored def was deleted, and
                    // blaming a surviving sibling would report a false
                    // "changed" (or silently rewrite the sourceMap).
                    None if entry.peers > 0 && (defs.len() as u32) < entry.peers => Fate::Broken,
                    None => Fate::Changed,
                }
            }
        } else {
            // No symbol (or no grammar): the recorded range first, then a
            // content search — an insertion above a line-only anchor MOVES it,
            // and before this search that always read as a false "changed"
            // (the re-anchor branch could never fire).
            let line_count = lines.len().max(1) as u32;
            let end_clamped = entry.end.min(line_count).max(entry.start);
            if span_hash(&lines, entry.start, end_clamped) == entry.hash {
                Fate::Quiet
            } else {
                let len = entry.end - entry.start + 1;
                match find_spans_by_hash(&lines, len, &entry.hash, 2)[..] {
                    [s] => Fate::Move(s, s + len - 1),
                    _ => Fate::Changed, // gone, or ambiguous duplicates
                }
            }
        };

        match fate {
            Fate::Quiet => {}
            Fate::Move(start, end) => {
                // Same content, new position: re-anchor silently. Update every
                // matching sourceMap location that recorded line numbers, and
                // the baseline span, so the lens stays sharp without flagging
                // anything.
                if let Some(locs) = keyed_locs_mut(&mut model, &entry.key) {
                    for l in locs.iter_mut() {
                        if l.pattern == entry.file && l.symbol == entry.symbol && l.line.is_some()
                        {
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
            Fate::Changed => out.push(observe(AnchorState::Changed, entry)),
            Fate::Broken => out.push(observe(AnchorState::Broken, entry)),
        }
    }

    if model_dirty {
        write_model_at(r, &model)?;
    }
    if baseline_dirty {
        let json = serde_json::to_string(&baseline).map_err(|e| e.to_string())?;
        let _ = std::fs::write(r.anchors_path(), json);
    }

    out.sort_by(|a, b| {
        (&a.host_id, &a.key, &a.file, &a.symbol).cmp(&(&b.host_id, &b.key, &b.file, &b.symbol))
    });
    // Dedup on symbol too: two sibling anchors of one claim in one file are
    // distinct observations, not duplicates.
    out.dedup_by(|a, b| {
        a.key == b.key && a.file == b.file && a.symbol == b.symbol && a.state == b.state
    });
    Ok(AnchorCheck {
        observations: out,
        reanchored,
    })
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

/// Every sourceMap location the current baseline holds no fingerprint for —
/// the file was absent at reconcile, the symbol unresolvable, or a glob
/// matched nothing. Empty when no baseline exists yet (the first reconcile
/// seeds it; nothing is meaningfully "untracked" before that).
pub fn untracked_anchors(r: &ModelRef) -> Result<Vec<UntrackedAnchor>, String> {
    let Some(baseline) = read_baseline(r) else {
        return Ok(Vec::new());
    };
    let model = read_model_at(r)?;
    let covered: HashSet<(&str, &str, Option<&str>)> = baseline
        .anchors
        .iter()
        .map(|e| (e.key.as_str(), e.source_pattern(), e.symbol.as_deref()))
        .collect();
    // Same two-dimension universe as write_baseline: a test anchor without a
    // fingerprint is a silent handle — "test-backed" must not read as watched
    // when the tripwire can't see the test.
    let mut keyed: Vec<(String, &Vec<scryer_core::SourceLocation>)> = model
        .source_map
        .iter()
        .map(|(k, v)| (k.clone(), v))
        .chain(model.test_map.iter().map(|(k, v)| (scryer_core::test_key(k), v)))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (key, locs) in keyed {
        for loc in locs {
            if !covered.contains(&(key.as_str(), loc.pattern.as_str(), loc.symbol.as_deref())) {
                out.push(UntrackedAnchor {
                    key: key.clone(),
                    file: loc.pattern.clone(),
                    symbol: loc.symbol.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// Out-of-plan regressions to already-mapped code — the live drift nudge.
///
/// Drift, mid-implementation, is a change the plan does NOT account for. The
/// plan is a positive description of the change we expect, so its footprint (the
/// elements `plan_diff_at` reports as added/reworded/moved/…) is the suppression
/// set: a committed anchor that breaks or changes BECAUSE the agent is
/// re-implementing the responsibility it backs is expected churn, not drift.
/// What survives is the dangerous case — a committed anchor that regressed with
/// no pending plan item to explain it (e.g. a method deleted that the plan never
/// touched). Rolled up per host node as [`drift::DriftScope`] so it feeds the
/// existing nudge unchanged.
///
/// Anchor-level by design: it only sees changes to code the model already maps,
/// which is exactly what keeps it cheap and quiet during planned work. Brand-new
/// undescribed behaviour has no committed anchor to trip and is the on-demand
/// semantic drift check's job, not this tripwire's.
pub fn out_of_plan_scopes(r: &ModelRef) -> Result<Vec<drift::DriftScope>, String> {
    // The plan's footprint, keyed by element. `obs.key` IS the element a broken
    // anchor backs — a responsibility id, or a node id for a schema declaration
    // — and `plan_diff_at` reports changes against those same ids, so a direct
    // key match is the precise test. Deliberately NOT by owning node: reworking
    // ONE responsibility must not silence a sibling regression on the same node
    // (the deleted-method case). Link/group/property ids simply never match an
    // anchor key, so collecting them all is harmless.
    let plan = scryer_core::plan_diff_at(r)?;
    let planned: HashSet<&str> = plan.changes.iter().map(|c| c.id.as_str()).collect();

    let check = check_anchors(r)?;
    // host_id → (host_name, changed files), so each node surfaces once.
    let mut by_host: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for obs in check.observations {
        if planned.contains(obs.key.as_str()) {
            continue;
        }
        // Test anchors are not drift inputs: an edited test is a health
        // signal (the claim's backing changed), not a regression to the
        // mapped implementation — and its key would never match a plan
        // element, so it could only ever add noise scopes here.
        if scryer_core::test_resp_id(&obs.key).is_some() {
            continue;
        }
        by_host
            .entry(obs.host_id)
            .or_insert_with(|| (obs.host_name, BTreeSet::new()))
            .1
            .insert(obs.file);
    }

    Ok(by_host
        .into_iter()
        .map(|(node_id, (node_name, files))| drift::DriftScope {
            node_id,
            node_name,
            changed_files: files.into_iter().collect(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{
        drift::SyncState, Kind, Node, Responsibility, ScryModel, SourceLocation,
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
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: vec![Responsibility {
                concern: None,
                id: "r1".into(),
                statement: "does the thing".into(),
                vagrant: None,
                stale: None,
                stale_proposal: None,
                directives: Vec::new(),
                last_touched_at: None,
            }],
            properties: Vec::new(),
            icon: None,
            visual: None,
            appearance: None,
            notes: None,
            position: None,
            directives: Vec::new(),
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
        scryer_core::write_sync_state(r, &SyncState::anchored_now(None)).unwrap();
        write_baseline(r).unwrap();
    }

    fn touch_gate() {
        // The sync anchor is nanosecond-precise; a token gap keeps the edit
        // strictly after it even on coarse-clock filesystems. (This used to be
        // 1100ms to clear whole-second mtime granularity — the same-second
        // blindness the ns anchor fixed.)
        std::thread::sleep(std::time::Duration::from_millis(25));
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

    /// Test anchors (claim → attached test) ride the same baseline under
    /// `test:{id}` keys: an edited test fires a namespaced observation that
    /// names the claim's host, and a test-anchor regression never mints a drift
    /// scope — it is a health signal, not an out-of-plan code regression.
    #[test]
    fn verify_anchor_rides_the_baseline_and_is_not_drift() {
        const TEST_TS: &str = "export function alphaSpec() {\n    return check(1);\n}\n";
        let (_dir, r) = project_with("src/m.ts", TS);
        std::fs::write(r.project_path().join("src/m.test.ts"), TEST_TS).unwrap();
        let mut m = leaf_model("alpha", "src/m.ts", 1, 3);
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.test.ts".into(),
                symbol: Some("alphaSpec".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        reconcile(&r);

        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.test.ts"),
            TEST_TS.replace("check(1)", "check(2)"),
        )
        .unwrap();

        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1, "{:?}", check.observations);
        let obs = &check.observations[0];
        assert_eq!(obs.key, "test:r1", "the observation is test-namespaced");
        assert_eq!(obs.host_id, "sym", "the host is the claim's, not the test's");
        assert_eq!(obs.state, AnchorState::Changed);

        let scopes = out_of_plan_scopes(&r).unwrap();
        assert!(scopes.is_empty(), "a test-anchor regression is not drift: {scopes:?}");
    }

    /// A test anchored by its NAME — `symbol` holding the `it("…")` string,
    /// not a code identifier — fingerprints and trips like any symbol anchor.
    /// (This is how agents record tests in practice; before test_blocks the
    /// baseline silently skipped these, so "test changed" could never fire.)
    #[test]
    fn name_string_test_anchor_is_fingerprinted_and_trips() {
        const SPEC_TS: &str = "describe(\"verify\", () => {\n  it(\"rejects an unsigned webhook\", async () => {\n    expect(check(1)).toBe(403);\n  });\n});\n";
        let (_dir, r) = project_with("src/m.ts", TS);
        std::fs::write(r.project_path().join("src/m.spec.ts"), SPEC_TS).unwrap();
        let mut m = leaf_model("alpha", "src/m.ts", 1, 3);
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.spec.ts".into(),
                symbol: Some("rejects an unsigned webhook".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        reconcile(&r);

        let un = untracked_anchors(&r).unwrap();
        assert!(
            !un.iter().any(|u| u.key == "test:r1"),
            "a name-anchored test is fingerprinted, not a silent handle: {un:?}"
        );

        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.spec.ts"),
            SPEC_TS.replace("check(1)", "check(2)"),
        )
        .unwrap();

        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1, "{:?}", check.observations);
        assert_eq!(check.observations[0].key, "test:r1");
        assert_eq!(check.observations[0].state, AnchorState::Changed);
    }

    /// A test entry added after the last reconcile has no fingerprint yet —
    /// the untracked sweep must surface it, or "test-backed" reads as watched
    /// while the tripwire is blind to the test.
    #[test]
    fn untracked_verify_anchor_surfaces() {
        let (_dir, r) = project_with("src/m.ts", TS);
        scryer_core::write_model_at(&r, &leaf_model("alpha", "src/m.ts", 1, 3)).unwrap();
        reconcile(&r);

        let mut m = read_model_at(&r).unwrap();
        m.test_map.insert(
            "r1".into(),
            vec![SourceLocation {
                pattern: "src/m.test.ts".into(),
                symbol: None,
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();

        let un = untracked_anchors(&r).unwrap();
        assert!(
            un.iter().any(|u| u.key == "test:r1" && u.file == "src/m.test.ts"),
            "silent test handle surfaces: {un:?}"
        );
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

    /// A line-only anchor (no symbol) survives insertions above it: the
    /// remembered content is FOUND in the file, not just re-read at the stale
    /// position — which used to guarantee a false `changed`.
    #[test]
    fn line_only_anchor_survives_insertion_above() {
        let src = "top\n\nconst A = 1;\nconst B = 2;\nconst C = 3;\n";
        let (_dir, r) = project_with("src/m.ts", src);
        let mut m = leaf_model("ignored", "src/m.ts", 3, 5);
        m.source_map.get_mut("r1").unwrap()[0].symbol = None;
        scryer_core::write_model_at(&r, &m).unwrap();
        reconcile(&r);

        touch_gate();
        std::fs::write(r.project_path().join("src/m.ts"), format!("// pad\n// pad\n{src}"))
            .unwrap();
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);
        assert_eq!(check.reanchored, 1);
        let m = read_model_at(&r).unwrap();
        let loc = &m.source_map["r1"][0];
        assert_eq!(loc.line, Some(5), "anchor followed its content down");
        assert_eq!(loc.end_line, Some(7));

        // Healed: quiet on the next check. A real edit still trips it.
        assert!(check_anchors(&r).unwrap().observations.is_empty());
        touch_gate();
        let padded = format!("// pad\n// pad\n{src}").replace("const B = 2;", "const B = 99;");
        std::fs::write(r.project_path().join("src/m.ts"), padded).unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::Changed);
    }

    /// A renamed file (mv preserves mtimes, so the walk can't see it) is
    /// rescued through the content hash: the sourceMap follows the file, no
    /// observation fires. Renamed AND edited stays fileMissing — never guess.
    #[test]
    fn renamed_file_is_rescued_by_content() {
        let (_dir, r) = project_with("src/m.ts", TS);
        scryer_core::write_model_at(&r, &leaf_model("alpha", "src/m.ts", 1, 3)).unwrap();
        reconcile(&r);

        std::fs::rename(
            r.project_path().join("src/m.ts"),
            r.project_path().join("src/renamed.ts"),
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);
        assert_eq!(check.reanchored, 1);
        let m = read_model_at(&r).unwrap();
        assert_eq!(m.source_map["r1"][0].pattern, "src/renamed.ts");
        assert!(check_anchors(&r).unwrap().observations.is_empty());

        // Rename + edit in one step: the remembered content exists nowhere.
        reconcile(&r);
        std::fs::remove_file(r.project_path().join("src/renamed.ts")).unwrap();
        std::fs::write(
            r.project_path().join("src/again.ts"),
            TS.replace("return 1;", "return 7;"),
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::FileMissing);
    }

    /// Hash-first symbol resolution: a same-named sibling appearing NEARER to
    /// the old position must not be adopted while the true (content-equal)
    /// def sits further down — nearest-wins used to re-anchor to the sibling.
    #[test]
    fn moved_symbol_outranks_nearer_same_named_sibling() {
        let v1 = "impl A {\n    fn parse(&self) -> u32 {\n        1\n    }\n}\n";
        let (_dir, r) = project_with("src/d.rs", v1);
        scryer_core::write_model_at(&r, &leaf_model("parse", "src/d.rs", 2, 4)).unwrap();
        reconcile(&r);

        touch_gate();
        // A different `parse` now occupies the OLD position; ours moved down.
        let v2 = "impl C {\n    fn parse(&self) -> u32 {\n        3\n    }\n}\nimpl A {\n    fn parse(&self) -> u32 {\n        1\n    }\n}\n";
        std::fs::write(r.project_path().join("src/d.rs"), v2).unwrap();
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);
        assert_eq!(check.reanchored, 1);
        let m = read_model_at(&r).unwrap();
        assert_eq!(
            m.source_map["r1"][0].line,
            Some(7),
            "re-anchored to the content-equal def, not the nearer sibling"
        );
    }

    /// Deleting the anchored def while a same-named sibling survives is
    /// BROKEN — not a `changed` misattributed to the sibling.
    #[test]
    fn deleted_def_with_surviving_sibling_is_broken() {
        let v1 = "impl A {\n    fn parse(&self) -> u32 {\n        1\n    }\n}\nimpl B {\n    fn parse(&self) -> u32 {\n        2\n    }\n}\n";
        let (_dir, r) = project_with("src/d.rs", v1);
        scryer_core::write_model_at(&r, &leaf_model("parse", "src/d.rs", 2, 4)).unwrap();
        reconcile(&r);

        touch_gate();
        // impl A's parse (the anchored one) is deleted; B's slides into the
        // exact old position with different content.
        let v2 = "impl B {\n    fn parse(&self) -> u32 {\n        2\n    }\n}\n";
        std::fs::write(r.project_path().join("src/d.rs"), v2).unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(
            check.observations[0].state,
            AnchorState::Broken,
            "population shrank with no content match — the anchored def is gone"
        );
    }

    /// A GLOB anchor claims territory: the baseline fingerprints every
    /// matched file, so an edit inside the territory trips the wire (these
    /// anchors used to fall out of the baseline silently), a rename within it
    /// heals quietly without touching the model's glob, and a deletion
    /// surfaces as fileMissing.
    #[test]
    fn glob_anchor_territory_is_fingerprinted() {
        let (_dir, r) = project_with("src/a.ts", "export function alpha() {\n    return 1;\n}\n");
        std::fs::write(
            r.project_path().join("src/b.ts"),
            "export function beta() {\n    return 2;\n}\n",
        )
        .unwrap();
        let mut m = leaf_model("ignored", "src/*.ts", 1, 1);
        m.source_map.get_mut("r1").unwrap()[0].symbol = None;
        m.source_map.get_mut("r1").unwrap()[0].line = None;
        m.source_map.get_mut("r1").unwrap()[0].end_line = None;
        scryer_core::write_model_at(&r, &m).unwrap();
        reconcile(&r);
        assert!(check_anchors(&r).unwrap().observations.is_empty());

        // Edit one matched file → changed, scoped to the concrete file.
        touch_gate();
        std::fs::write(
            r.project_path().join("src/b.ts"),
            "export function beta() {\n    return 99;\n}\n",
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::Changed);
        assert_eq!(check.observations[0].file, "src/b.ts");

        // Rename within the territory → healed silently, the glob loc intact.
        reconcile(&r);
        std::fs::rename(
            r.project_path().join("src/b.ts"),
            r.project_path().join("src/b2.ts"),
        )
        .unwrap();
        let check = check_anchors(&r).unwrap();
        assert!(check.observations.is_empty(), "{:?}", check.observations);
        assert_eq!(check.reanchored, 1);
        let m = read_model_at(&r).unwrap();
        assert_eq!(
            m.source_map["r1"][0].pattern, "src/*.ts",
            "the model still spells the glob"
        );

        // Delete a matched file → fileMissing for that file.
        reconcile(&r);
        std::fs::remove_file(r.project_path().join("src/b2.ts")).unwrap();
        let check = check_anchors(&r).unwrap();
        assert_eq!(check.observations.len(), 1);
        assert_eq!(check.observations[0].state, AnchorState::FileMissing);
        assert_eq!(check.observations[0].file, "src/b2.ts");
    }

    /// Anchors the baseline could not fingerprint are SILENT — no drift ever
    /// fires for them — and must be reported, not read as green.
    #[test]
    fn untracked_anchors_surface_silent_spans() {
        let (_dir, r) = project_with("src/m.ts", TS);
        let mut m = leaf_model("alpha", "src/m.ts", 1, 3);
        // A second claim anchored to a file that does not exist: write_baseline
        // skips it (nothing to remember) — exactly the silent case.
        m.nodes[0].responsibilities.push(Responsibility {
            concern: None,
            id: "r2".into(),
            statement: "claims ghost code".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.source_map.insert(
            "r2".into(),
            vec![SourceLocation {
                pattern: "src/ghost.ts".into(),
                symbol: Some("phantom".into()),
                line: None,
                end_line: None,
                command: None,
            }],
        );
        scryer_core::write_model_at(&r, &m).unwrap();
        reconcile(&r);

        let untracked = untracked_anchors(&r).unwrap();
        assert_eq!(untracked.len(), 1, "{untracked:?}");
        assert_eq!(untracked[0].key, "r2");
        assert_eq!(untracked[0].file, "src/ghost.ts");
        // The healthy anchor is fingerprinted, hence not reported.
        assert!(!untracked.iter().any(|u| u.key == "r1"));
    }

    /// Two responsibilities on one node, anchored to `alpha` and `beta`. The
    /// plan reworks r1 (alpha) only. A change to alpha is expected churn —
    /// suppressed; a change to beta has no pending plan item — it surfaces. This
    /// is the deleted-method case: out-of-plan regressions tick even while the
    /// node has other planned work in flight.
    fn two_resp_model() -> ScryModel {
        let mut m = leaf_model("alpha", "src/m.ts", 1, 3);
        // leaf_model gives node "sym" + r1→alpha; add r2→beta on the same node.
        m.nodes[0].responsibilities.push(Responsibility {
            concern: None,
            id: "r2".into(),
            statement: "does beta".into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        });
        m.source_map.insert(
            "r2".into(),
            vec![SourceLocation {
                pattern: "src/m.ts".into(),
                symbol: Some("beta".into()),
                line: Some(5),
                end_line: Some(7),
                command: None,
            }],
        );
        m
    }

    #[test]
    fn out_of_plan_scopes_suppresses_planned_surfaces_unplanned() {
        let (_dir, r) = project_with("src/m.ts", TS);
        let committed = two_resp_model();
        scryer_core::write_model_at(&r, &committed).unwrap();

        // Plan reworks r1's statement; r2 is untouched by the plan.
        let mut planned = committed.clone();
        planned.nodes[0].responsibilities[0].statement = "does alpha, revised".into();
        scryer_core::write_planned_at(&r, &planned).unwrap();
        reconcile(&r);

        // Edit alpha (the planned responsibility's code) → expected, suppressed.
        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.ts"),
            TS.replace(
                "function alpha() {\n    return 1;",
                "function alpha() {\n    return 42;",
            ),
        )
        .unwrap();
        assert!(
            out_of_plan_scopes(&r).unwrap().is_empty(),
            "a change the plan accounts for is not drift"
        );

        // Now delete beta — a regression no pending plan item explains.
        touch_gate();
        std::fs::write(
            r.project_path().join("src/m.ts"),
            "export function alpha() {\n    return 42;\n}\n",
        )
        .unwrap();
        let scopes = out_of_plan_scopes(&r).unwrap();
        assert_eq!(scopes.len(), 1, "the out-of-plan deletion surfaces");
        assert_eq!(scopes[0].node_id, "sym");
        assert!(scopes[0].changed_files.iter().any(|f| f == "src/m.ts"));
    }
}
