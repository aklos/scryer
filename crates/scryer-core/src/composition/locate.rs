//! Looking a file up in the model: read both layers, then answer.

use crate::application::locate::*;
use crate::composition::model_store::{plan_diff_at, read_model_at, read_planned_at};

/// Resolve `file` against the project's WORKING view (plan + committed anchors)
/// and scope the plan diff to what was located. `file` must already be
/// project-relative and `/`-separated.
pub fn locate_at(
    r: &crate::ModelRef,
    file: &str,
    symbol: Option<&str>,
) -> Result<LocateReport, String> {
    let committed = read_model_at(r)?;
    let planned = read_planned_at(r)?;
    let working = crate::working_view(&committed, &planned);

    let result = locate(&working, file, symbol);

    let mut located_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for o in &result.owner_chain {
        located_ids.insert(&o.id);
    }
    if let Some(b) = &result.boundary_owner {
        located_ids.insert(&b.id);
    }
    for c in &result.claims {
        located_ids.insert(&c.id);
        located_ids.insert(&c.host_id);
    }
    let pending: Vec<crate::domain::diff::ElementChange> = plan_diff_at(r)?
        .changes
        .into_iter()
        .filter(|c| {
            located_ids.contains(c.id.as_str())
                || c.owner_id.as_deref().is_some_and(|o| located_ids.contains(o))
        })
        .collect();

    let path = (!result.owner_chain.is_empty()).then(|| {
        result
            .owner_chain
            .iter()
            .rev()
            .map(|o| o.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    });

    let styles = crate::composition::styles::load_styles(r.project_path());
    let placement = result
        .owner_chain
        .first()
        .map(|o| o.id.as_str())
        .or_else(|| result.boundary_owner.as_ref().map(|b| b.id.as_str()))
        .and_then(|id| crate::domain::style::placement(&working, &styles, r.project_path(), id));

    Ok(LocateReport { result, path, pending, placement })
}
