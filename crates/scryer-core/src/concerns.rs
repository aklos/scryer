//! Cross-cutting concern vocabulary — the model's third axis.
//!
//! The containment tree answers "what's inside X?", links answer "what talks
//! to what?"; a concern answers "where does auth (persistence, idempotency, …)
//! live?". Code answers that question with factoring plus convention
//! (`middleware/`, decorators); a model can't relocate prose, so the concern is
//! a LENS instead: each responsibility may carry at most one concern slug, and
//! the registry on [`crate::ScryModel`] names every slug in use so the UI can
//! group, badge, and dim by it.
//!
//! Registry entries are minted automatically on first use ([`register_concerns`]
//! runs on every model write) and never pruned automatically — a registered
//! concern with zero tagged responsibilities is itself signal ("this app has no
//! auth story"), and explicit deletion is a user act. Slugs are normalized to
//! kebab-case on write so the vocabulary can't fork on casing or spacing.

use serde::{Deserialize, Serialize};

use crate::model::ScryModel;

/// One entry in the model's concern registry: the single place a concern is
/// named and decorated. Responsibilities reference it by `slug`; renaming a
/// concern means rewriting the slug here AND on every tagged responsibility
/// (the registry entry is the concept, not per-responsibility text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConcernDef {
    /// Kebab-case identifier, e.g. "auth", "failure-handling". Displayed as-is.
    pub slug: String,
    /// One line on what accountability the concern covers. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Lucide icon name (PascalCase, e.g. "Shield") — the glyph that prefixes
    /// every responsibility tagged with this concern. Falls back to a
    /// deterministic pick from `slug` when unset. Frontend-only meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// The seeded standard vocabulary: `(slug, description, icon)`. A used slug
/// that matches one of these gets its description and icon filled in when the
/// registry entry is minted; the set also anchors the agent's tagging rule
/// (rules.rs rule 20) so every model answers the same questions the same way.
pub const STANDARD_CONCERNS: &[(&str, &str, &str)] = &[
    ("auth", "Identity, authentication, and access control", "Shield"),
    ("persistence", "Durable storage and retrieval of data", "Database"),
    ("failure-handling", "Detecting, capturing, and recovering from failures", "AlertTriangle"),
    ("idempotency", "Making retries and duplicate deliveries safe", "Repeat"),
    ("validation", "Checking inputs against expected shape and rules", "CheckCircle"),
    ("observability", "Logging, metrics, and tracing for runtime insight", "Activity"),
    ("performance", "Speed, capacity, and resource efficiency", "Gauge"),
    ("compliance", "Satisfying external policy, legal, or platform rules", "Scale"),
];

/// Normalize a raw concern value to a kebab-case slug: lowercase, every run of
/// non-alphanumerics collapses to one hyphen, leading/trailing hyphens trimmed.
/// Returns an empty string for input with no alphanumerics — callers treat
/// that as "no concern".
pub fn normalize_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut pending_hyphen = false;
    for c in raw.chars() {
        if c.is_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.extend(c.to_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    out
}

/// Normalize every responsibility's concern slug and make sure each used slug
/// has a registry entry — the single choke point that keeps tags and registry
/// coherent. Runs on both model write paths ([`crate::write_model_at`] /
/// [`crate::write_planned_at`]), so agent writes can never leave a dangling or
/// misspelled-case slug. New entries seed description/icon from
/// [`STANDARD_CONCERNS`] when the slug matches; existing entries are never
/// touched and unused entries are never pruned. The registry is kept sorted by
/// slug for stable serialization. Mirrored in the frontend mutation helpers
/// (`src/viewmodel.ts`), which write raw JSON and bypass this path — keep the
/// two in lockstep.
pub fn register_concerns(model: &mut ScryModel) {
    let mut used: Vec<String> = Vec::new();
    let mut visit = |resp_concern: &mut Option<String>| {
        if let Some(raw) = resp_concern.as_deref() {
            let slug = normalize_slug(raw);
            if slug.is_empty() {
                *resp_concern = None;
            } else {
                if !used.contains(&slug) {
                    used.push(slug.clone());
                }
                *resp_concern = Some(slug);
            }
        }
    };
    for n in &mut model.nodes {
        for r in &mut n.responsibilities {
            visit(&mut r.concern);
        }
    }
    for g in &mut model.groups {
        for r in &mut g.responsibilities {
            visit(&mut r.concern);
        }
    }

    for slug in used {
        if model.concerns.iter().any(|c| c.slug == slug) {
            continue;
        }
        let std = STANDARD_CONCERNS.iter().find(|(s, _, _)| *s == slug);
        model.concerns.push(ConcernDef {
            slug,
            description: std.map(|(_, d, _)| d.to_string()),
            icon: std.map(|(_, _, i)| i.to_string()),
        });
    }
    model.concerns.sort_by(|a, b| a.slug.cmp(&b.slug));
}

/// Metadata write-through, plan → committed. A concern tag has no build
/// semantics, so a retag must not wait for a fold that will never come (`diff`
/// deliberately ignores `concern`, so a tag-only change creates no work item).
/// Instead, every plan write syncs the tag onto the committed copy of any
/// claim that exists there (matched by id), and copies the registry wholesale
/// — the plan is the authoring surface for curation (renames, descriptions,
/// icons). Returns whether `committed` changed and needs writing. Runs inside
/// [`crate::write_planned_raw_at`], the choke point both the canvas and the
/// MCP write path go through.
pub fn sync_concern_metadata(committed: &mut ScryModel, planned: &ScryModel) -> bool {
    let mut changed = false;
    let planned_by_id: std::collections::HashMap<&str, Option<&str>> = planned
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(planned.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|r| (r.id.as_str(), r.concern.as_deref()))
        .collect();
    let committed_resps = committed
        .nodes
        .iter_mut()
        .flat_map(|n| n.responsibilities.iter_mut())
        .chain(committed.groups.iter_mut().flat_map(|g| g.responsibilities.iter_mut()));
    for r in committed_resps {
        if let Some(&tag) = planned_by_id.get(r.id.as_str()) {
            if r.concern.as_deref() != tag {
                r.concern = tag.map(Into::into);
                changed = true;
            }
        }
    }
    if committed.concerns != planned.concerns {
        committed.concerns = planned.concerns.clone();
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Node, Responsibility};

    fn resp(id: &str, concern: Option<&str>) -> Responsibility {
        Responsibility {
            id: id.into(),
            statement: "does a thing".into(),
            concern: concern.map(Into::into),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    fn model_with(resps: Vec<Responsibility>) -> ScryModel {
        let mut m = ScryModel::default();
        m.nodes.push(Node {
            id: "n1".into(),
            kind: Kind::Component,
            name: "N".into(),
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            vagrant: None,
            stale: None,
            responsibilities: resps,
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        });
        m
    }

    #[test]
    fn normalize_slug_kebabs_and_collapses() {
        assert_eq!(normalize_slug("Auth"), "auth");
        assert_eq!(normalize_slug("Failure  Handling"), "failure-handling");
        assert_eq!(normalize_slug("--rate__limiting--"), "rate-limiting");
        assert_eq!(normalize_slug("  "), "");
    }

    #[test]
    fn register_mints_entries_seeding_standards_and_normalizes_tags() {
        let mut m = model_with(vec![
            resp("r1", Some("Auth")),
            resp("r2", Some("session-windows")),
            resp("r3", None),
            resp("r4", Some("!!")), // normalizes to empty → cleared
        ]);
        register_concerns(&mut m);

        let node = &m.nodes[0];
        assert_eq!(node.responsibilities[0].concern.as_deref(), Some("auth"));
        assert_eq!(node.responsibilities[3].concern, None);

        // Registry: standard slug got description+icon, custom slug got bare entry.
        let auth = m.concerns.iter().find(|c| c.slug == "auth").unwrap();
        assert_eq!(auth.icon.as_deref(), Some("Shield"));
        assert!(auth.description.is_some());
        let custom = m.concerns.iter().find(|c| c.slug == "session-windows").unwrap();
        assert_eq!(custom.icon, None);
        // Sorted by slug.
        assert_eq!(
            m.concerns.iter().map(|c| c.slug.as_str()).collect::<Vec<_>>(),
            vec!["auth", "session-windows"]
        );
    }

    #[test]
    fn sync_writes_tags_and_registry_through_to_committed() {
        // Committed holds the built claim untagged; the plan retagged it and
        // minted the registry entry. Sync carries both across — and leaves the
        // plan-only claim (unknown to committed) alone.
        let mut committed = model_with(vec![resp("r1", None), resp("r2", Some("auth"))]);
        let mut planned = model_with(vec![resp("r1", Some("auth")), resp("r2", None)]);
        planned.nodes[0].responsibilities.push(resp("r3", Some("persistence")));
        register_concerns(&mut planned);

        assert!(sync_concern_metadata(&mut committed, &planned));
        let rs = &committed.nodes[0].responsibilities;
        assert_eq!(rs[0].concern.as_deref(), Some("auth")); // tagged through
        assert_eq!(rs[1].concern, None); // untagged through
        assert_eq!(committed.concerns, planned.concerns); // registry copied

        // Idempotent: a second sync finds nothing to do.
        assert!(!sync_concern_metadata(&mut committed, &planned));
    }

    #[test]
    fn register_never_touches_existing_entries_or_prunes_unused() {
        let mut m = model_with(vec![resp("r1", Some("auth"))]);
        m.concerns.push(ConcernDef {
            slug: "auth".into(),
            description: Some("user-curated wording".into()),
            icon: Some("Lock".into()),
        });
        m.concerns.push(ConcernDef { slug: "unused".into(), description: None, icon: None });
        register_concerns(&mut m);

        let auth = m.concerns.iter().find(|c| c.slug == "auth").unwrap();
        assert_eq!(auth.description.as_deref(), Some("user-curated wording"));
        assert_eq!(auth.icon.as_deref(), Some("Lock"));
        assert!(m.concerns.iter().any(|c| c.slug == "unused"));
    }
}
