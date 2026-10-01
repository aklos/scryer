//! What the session hooks decide, given the model and the session's log.
//! Pure — the hook client reads the files, runs the anchor check and appends
//! the events these answers call for.

use crate::application::locate::locate;
use crate::domain::model::ScryModel;
use crate::domain::session::{AskKind, SessionLog};
use serde::Serialize;
use std::collections::HashMap;

/// One anchor the fingerprint check reports out of sync, in the shape the close
/// view needs. The check itself lives outside this crate.
#[derive(Debug, Clone)]
pub struct AnchorFlag {
    pub key: String,
    pub host_name: String,
    pub file: String,
    pub symbol: Option<String>,
    /// `changed`, `broken` or `fileMissing`.
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedClaim {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// An anchor state, or `unreconciled` for an anchor with no baseline.
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileClaims {
    pub file: String,
    pub claims: Vec<FlaggedClaim>,
}

/// The close-gate view, anchor-informed so it gates only what is genuinely out
/// of sync. Touched files partition three ways:
///
/// - `needs_reconcile` — files carrying a claim the check can't vouch for:
///   a committed anchor whose fingerprint reports changed / broken / missing,
///   or a plan-added / glob anchor with no baseline to fingerprint at all.
/// - `clean_modeled` — files whose only claims are committed anchors that hash
///   clean: the session edited around the modeled behaviour and owes nothing.
/// - `unmodeled` — files the model doesn't map at all.
///
/// The fingerprint check compares against the last reconcile baseline, so a
/// long-unreconciled file may surface pre-session changes too — still the right
/// call: the claim needs a look and this session just worked there.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloseView {
    pub needs_reconcile: Vec<FileClaims>,
    pub clean_modeled: Vec<String>,
    pub unmodeled: Vec<String>,
}

pub fn close_view(
    committed: Option<&ScryModel>,
    working: Option<&ScryModel>,
    flags: &[AnchorFlag],
    touched: &[String],
) -> CloseView {
    let statement_of = |key: &str| -> Option<String> {
        // A test-anchor key names the claim its test backs.
        let key = crate::test_resp_id(key).unwrap_or(key);
        let w = working?;
        w.nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(w.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .find(|resp| resp.id == key)
            .map(|resp| resp.statement.clone())
    };
    let host_name_of = |key: &str| -> Option<String> {
        let w = working?;
        for n in &w.nodes {
            if n.id == key || n.responsibilities.iter().any(|resp| resp.id == key) {
                return Some(n.name.clone());
            }
        }
        w.groups
            .iter()
            .find(|g| g.responsibilities.iter().any(|resp| resp.id == key))
            .map(|g| g.name.clone())
    };
    let loc_matches = |pattern: &str, file: &str| -> bool {
        pattern == file || glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file))
    };
    // The fingerprint baseline covers only committed sourceMap keys with an
    // EXACT location for the file. Anything else on the file — a plan-added
    // anchor or a glob location (never fingerprinted) — can't be verified, so
    // a touch surfaces it for a look rather than passing it as clean.
    let committed_exact = |key: &str, file: &str| -> bool {
        committed.is_some_and(|c| {
            c.source_map
                .get(key)
                .is_some_and(|locs| locs.iter().any(|l| l.pattern == file))
        })
    };

    let mut view = CloseView::default();
    for file in touched {
        let file = file.as_str();
        // 1) Committed anchors the fingerprint check flagged.
        let mut dirty: Vec<FlaggedClaim> = flags
            .iter()
            .filter(|f| f.file == file)
            .map(|f| FlaggedClaim {
                id: f.key.clone(),
                host: Some(f.host_name.clone()),
                symbol: f.symbol.clone(),
                state: f.state.clone(),
                statement: statement_of(&f.key),
            })
            .collect();

        // 2) Plan-added and glob anchors on this file — unverifiable.
        if let Some(w) = working {
            let mut keys: Vec<&String> = w.source_map.keys().collect();
            keys.sort();
            for key in keys {
                if committed_exact(key, file) {
                    continue;
                }
                if let Some(loc) = w.source_map[key].iter().find(|l| loc_matches(&l.pattern, file)) {
                    dirty.push(FlaggedClaim {
                        id: key.clone(),
                        host: host_name_of(key),
                        symbol: loc.symbol.clone(),
                        state: "unreconciled".into(),
                        statement: statement_of(key),
                    });
                }
            }
        }

        if !dirty.is_empty() {
            view.needs_reconcile.push(FileClaims { file: file.to_string(), claims: dirty });
        } else if working.is_some_and(|w| !locate(w, file, None).claims.is_empty()) {
            view.clean_modeled.push(file.to_string());
        } else {
            view.unmodeled.push(file.to_string());
        }
    }
    view
}

/// Where one ask stands, judged against the model and the verdicts.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AskStatus {
    Delivered,
    Answered,
    Descoped { reason: String },
    /// What is still missing, one item per gap.
    Open { missing: Vec<String> },
}

impl AskStatus {
    pub fn is_open(&self) -> bool {
        matches!(self, AskStatus::Open { .. })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AskView {
    pub id: String,
    pub prompt: String,
    pub text: String,
    pub kind: AskKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub claims: Vec<String>,
    #[serde(flatten)]
    pub status: AskStatus,
}

fn pattern_matches(pattern: &str, file: &str) -> bool {
    pattern == file || glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file))
}

/// The files a claim's code and tests live in, as anchored in `working`.
fn claim_patterns<'a>(working: &'a ScryModel, claim: &str) -> impl Iterator<Item = &'a str> {
    working
        .source_map
        .get(claim)
        .into_iter()
        .flatten()
        .chain(working.test_map.get(claim).into_iter().flatten())
        .map(|l| l.pattern.as_str())
}

/// Judge every ask in the log. A build ask is delivered when it has claims and
/// each one exists, has a passing verdict (`verified`), and anchors code this
/// session edited — a green suite alone never delivers anything.
pub fn ask_views(
    log: &SessionLog,
    working: &ScryModel,
    verified: &HashMap<String, bool>,
) -> Vec<AskView> {
    let exists = |id: &str| {
        working
            .nodes
            .iter()
            .flat_map(|n| n.responsibilities.iter())
            .chain(working.groups.iter().flat_map(|g| g.responsibilities.iter()))
            .any(|r| r.id == id)
    };
    log.asks
        .iter()
        .map(|a| {
            let status = if let Some(reason) = &a.descoped {
                AskStatus::Descoped { reason: reason.clone() }
            } else {
                match a.ask.kind {
                    AskKind::Answer if a.answered => AskStatus::Answered,
                    AskKind::Answer => AskStatus::Open {
                        missing: vec![format!("not answered yet — resolve_ask {{id: \"{}\", answered: true}} once it is", a.ask.id)],
                    },
                    AskKind::Build => {
                        let mut missing = Vec::new();
                        if a.claims.is_empty() {
                            missing.push(format!(
                                "no claim delivers it — model it, then resolve_ask {{id: \"{}\", claims: [...]}}",
                                a.ask.id
                            ));
                        }
                        for c in &a.claims {
                            if !exists(c) {
                                missing.push(format!("{c} is not in the model"));
                                continue;
                            }
                            if !verified.get(c).copied().unwrap_or(false) {
                                missing.push(format!("{c} has no passing test verdict"));
                            }
                            let touched = claim_patterns(working, c)
                                .any(|p| log.touched.iter().any(|f| pattern_matches(p, f)));
                            if !touched {
                                missing.push(format!("{c}: this session edited none of its anchored code"));
                            }
                        }
                        if missing.is_empty() {
                            AskStatus::Delivered
                        } else {
                            AskStatus::Open { missing }
                        }
                    }
                }
            };
            AskView {
                id: a.ask.id.clone(),
                prompt: a.prompt.clone(),
                text: a.ask.text.clone(),
                kind: a.ask.kind,
                source: a.ask.source.clone(),
                claims: a.claims.clone(),
                status,
            }
        })
        .collect()
}

/// Files this session edited that no ask accounts for: not anchored by (or a
/// test of) any linked claim, and not an ask's `source`. "I didn't ask for that."
pub fn untraced_edits(log: &SessionLog, working: &ScryModel) -> Vec<String> {
    log.touched
        .iter()
        .filter(|f| {
            !log.asks.iter().any(|a| {
                a.ask.source.as_deref().is_some_and(|s| pattern_matches(s, f) || f.starts_with(s))
                    || a.claims
                        .iter()
                        .any(|c| claim_patterns(working, c).any(|p| pattern_matches(p, f)))
            })
        })
        .cloned()
        .collect()
}

/// What the Stop gate blocks on now: prompts never broken into asks and open
/// asks — each only if it was never blocked on before.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AsksGate {
    pub prompts: Vec<(String, String)>,
    pub asks: Vec<AskView>,
}

impl AsksGate {
    pub fn is_empty(&self) -> bool {
        self.prompts.is_empty() && self.asks.is_empty()
    }

    /// The block reason, addressed to the agent. It names exactly what is left
    /// and never asks for the user's review.
    pub fn reason(&self) -> String {
        let mut out = String::from("Scryer ask ledger — not done yet:\n");
        for (id, text) in &self.prompts {
            out.push_str(&format!(
                "- prompt {id} was never broken into asks: \"{}\" — file_asks {{prompt: \"{id}\", asks: [...]}} (an empty list if it asked for nothing new)\n",
                clip(text, 160)
            ));
        }
        for a in &self.asks {
            out.push_str(&format!("- {} \"{}\":\n", a.id, clip(&a.text, 120)));
            if let AskStatus::Open { missing } = &a.status {
                for m in missing {
                    out.push_str(&format!("    {m}\n"));
                }
            }
        }
        out.push_str(
            "Finish each one. If one truly cannot or should not be done, resolve_ask {id, descoped: \"<one-line reason>\"} — \
             the user sees the reason. This gate does not fire again for these items.",
        );
        out
    }
}

pub fn asks_gate(log: &SessionLog, views: &[AskView]) -> AsksGate {
    AsksGate {
        prompts: log
            .unfiled_prompts()
            .into_iter()
            .filter(|p| !log.gated_prompts.iter().any(|g| g == p))
            .filter_map(|p| log.prompts.iter().find(|(id, _)| id == p).cloned())
            .collect(),
        asks: views
            .iter()
            .filter(|v| v.status.is_open() && !log.gated_asks.contains(&v.id))
            .cloned()
            .collect(),
    }
}

/// The one-line summary for the user, or `None` when there is nothing to say.
pub fn session_summary(views: &[AskView], untraced: &[String]) -> Option<String> {
    if views.is_empty() && untraced.is_empty() {
        return None;
    }
    let count = |f: &dyn Fn(&AskStatus) -> bool| views.iter().filter(|v| f(&v.status)).count();
    let done = count(&|s| matches!(s, AskStatus::Delivered | AskStatus::Answered));
    let open = count(&|s| s.is_open());
    let mut parts = vec![format!("asks {done}/{} done", views.len())];
    if open > 0 {
        let ids: Vec<&str> = views.iter().filter(|v| v.status.is_open()).map(|v| v.id.as_str()).collect();
        parts.push(format!("{open} open ({})", ids.join(", ")));
    }
    for v in views {
        if let AskStatus::Descoped { reason } = &v.status {
            parts.push(format!("descoped {} \"{}\": {}", v.id, clip(&v.text, 60), clip(reason, 100)));
        }
    }
    if !untraced.is_empty() {
        let shown: Vec<&str> = untraced.iter().take(5).map(String::as_str).collect();
        let more = untraced.len().saturating_sub(5);
        parts.push(format!(
            "edits no ask accounts for: {}{}",
            shown.join(", "),
            if more > 0 { format!(" +{more}") } else { String::new() }
        ));
    }
    Some(format!("scryer · {}", parts.join(" · ")))
}

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// System > Container with a claim anchored in src/auth.rs.
    fn model() -> ScryModel {
        serde_json::from_value(serde_json::json!({
            "version": "0.3", "links": [],
            "nodes": [
                { "id": "sys", "kind": "system", "name": "Acme" },
                { "id": "api", "kind": "container", "name": "API", "parentId": "sys",
                  "responsibilities": [{ "id": "r-1", "statement": "serves requests" }] }
            ],
            "sourceMap": { "r-1": [{ "pattern": "src/auth.rs", "symbol": "verify" }] }
        }))
        .unwrap()
    }

    fn touched(files: &[&str]) -> Vec<String> {
        files.iter().map(|f| f.to_string()).collect()
    }

    #[test]
    fn a_flagged_anchor_needs_reconcile_and_an_unflagged_one_is_clean() {
        let m = model();
        let clean = close_view(Some(&m), Some(&m), &[], &touched(&["src/auth.rs", "README.md"]));
        assert!(clean.needs_reconcile.is_empty());
        assert_eq!(clean.clean_modeled, vec!["src/auth.rs"]);
        assert_eq!(clean.unmodeled, vec!["README.md"]);

        let flag = AnchorFlag {
            key: "r-1".into(),
            host_name: "API".into(),
            file: "src/auth.rs".into(),
            symbol: Some("verify".into()),
            state: "changed".into(),
        };
        let dirty = close_view(Some(&m), Some(&m), &[flag], &touched(&["src/auth.rs"]));
        let claim = &dirty.needs_reconcile[0].claims[0];
        assert_eq!(claim.id, "r-1");
        assert_eq!(claim.state, "changed");
        assert_eq!(claim.statement.as_deref(), Some("serves requests"));
    }

    /// A claim authored and anchored in the plan has no baseline to
    /// fingerprint; touching its file surfaces it as `unreconciled`.
    #[test]
    fn a_plan_authored_anchor_is_unreconciled() {
        let committed = model();
        let mut plan = committed.clone();
        plan.nodes[1].responsibilities.push(
            serde_json::from_value(serde_json::json!({ "id": "r-2", "statement": "new plan claim" })).unwrap(),
        );
        plan.source_map.insert(
            "r-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "src/new.rs", "symbol": "foo" })).unwrap()],
        );
        let working = crate::working_view(&committed, &plan);
        let v = close_view(Some(&committed), Some(&working), &[], &touched(&["src/new.rs"]));
        assert_eq!(v.needs_reconcile[0].file, "src/new.rs");
        let claim = &v.needs_reconcile[0].claims[0];
        assert_eq!((claim.id.as_str(), claim.state.as_str()), ("r-2", "unreconciled"));
        assert_eq!(claim.host.as_deref(), Some("API"));
    }
}
