//! What the session hooks decide, given the model and the session's log.
//! Pure — the hook client reads the files, runs the anchor check and appends
//! the events these answers call for.

use crate::application::locate::locate;
use crate::domain::model::ScryModel;
use serde::Serialize;

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
