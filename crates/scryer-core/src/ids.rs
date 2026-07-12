use crate::model::{Responsibility, ScryModel};

// --- ID helpers ---

/// Highest numeric suffix among ids carrying `prefix` (e.g. `node-`); 0 if none.
pub fn max_id_suffix<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str) -> u64 {
    ids.filter_map(|id| id.strip_prefix(prefix).and_then(|s| s.parse::<u64>().ok()))
        .max()
        .unwrap_or(0)
}

pub fn next_node_id(model: &ScryModel) -> String {
    let max = max_id_suffix(model.nodes.iter().map(|n| n.id.as_str()), "node-");
    format!("node-{}", max + 1)
}

pub fn make_link_id(src: &str, dst: &str) -> String {
    format!("link-{}-{}", src, dst)
}

pub fn next_link_id(model: &ScryModel) -> String {
    let max = max_id_suffix(model.links.iter().map(|l| l.id.as_str()), "link-");
    format!("link-{}", max + 1)
}

pub fn next_group_id(model: &ScryModel) -> String {
    let max = max_id_suffix(model.groups.iter().map(|g| g.id.as_str()), "group-");
    format!("group-{}", max + 1)
}

pub fn next_responsibility_id(existing: &[Responsibility]) -> String {
    let max = max_id_suffix(existing.iter().map(|r| r.id.as_str()), "resp-");
    format!("resp-{}", max + 1)
}

/// Next `node-N` id past BOTH the planned draft and the committed model, so a
/// node the plan deleted (still live in committed) can't have its id re-issued:
/// the pending deletion would then read as a reword and `mark_implemented`
/// would overwrite the committed node.
pub fn next_node_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    let max = max_id_suffix(
        planned.nodes.iter().chain(committed.nodes.iter()).map(|n| n.id.as_str()),
        "node-",
    );
    format!("node-{}", max + 1)
}

/// Next `group-N` id past both layers — same union guard as [`next_node_id_union`].
pub fn next_group_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    let max = max_id_suffix(
        planned.groups.iter().chain(committed.groups.iter()).map(|g| g.id.as_str()),
        "group-",
    );
    format!("group-{}", max + 1)
}
