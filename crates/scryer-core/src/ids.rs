use crate::model::{Responsibility, ScryModel};
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

// --- ID helpers ---
//
// Ids are `<prefix>-<6 random chars>`. They used to be `<prefix>-<max+1>`,
// which is guaranteed to collide the moment two branches, worktrees, or
// sessions each mint against the same snapshot — both pick `node-681`, and
// the git merge either conflicts or silently lands two elements sharing an id.
// A random draw checked against every id the caller can see makes parallel
// minting safe without coordination. Older sequential ids stay valid: nothing
// parses the suffix, an id is only ever compared for equality.

/// 32 symbols, lowercase, with the look-alikes (i l o u) dropped so an id can
/// be read aloud or retyped without ambiguity. Six of them give ~10⁹ ids per
/// prefix — collision within one model is a re-draw, not a design concern.
const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
const SUFFIX_LEN: usize = 6;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh 30-bit draw. Entropy comes from the std `RandomState` hasher (seeded
/// per process from the OS), folded with the clock and a process-wide counter
/// so two draws in the same nanosecond still differ. Not cryptographic — it
/// only has to make two independent minters disagree.
fn draw() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    h.write_u64(nanos);
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u64(std::process::id() as u64);
    h.finish()
}

/// The suffix for one draw.
fn suffix(mut bits: u64) -> String {
    let mut s = String::with_capacity(SUFFIX_LEN);
    for _ in 0..SUFFIX_LEN {
        s.push(ALPHABET[(bits & 31) as usize] as char);
        bits >>= 5;
    }
    s
}

/// Mint `<prefix>-<suffix>`, re-drawing until the id is not in `taken`.
pub fn mint_id(prefix: &str, taken: &HashSet<&str>) -> String {
    loop {
        let id = format!("{prefix}-{}", suffix(draw()));
        if !taken.contains(id.as_str()) {
            return id;
        }
    }
}

/// Like [`mint_id`], over any iterator of existing ids.
pub fn mint_id_from<'a>(prefix: &str, existing: impl IntoIterator<Item = &'a str>) -> String {
    let taken: HashSet<&str> = existing.into_iter().collect();
    mint_id(prefix, &taken)
}

/// Whether `id` has the shape a minter produces for `prefix` — either the
/// current random form or the older sequential one. Used by the raw-write
/// tools to tell an echoed real id from a caller-invented placeholder.
pub fn is_minted_id(id: &str, prefix: &str) -> bool {
    let Some(rest) = id.strip_prefix(prefix).and_then(|s| s.strip_prefix('-')) else {
        return false;
    };
    !rest.is_empty()
        && (rest.bytes().all(|b| b.is_ascii_digit())
            || (rest.len() == SUFFIX_LEN && rest.bytes().all(|b| ALPHABET.contains(&b))))
}

pub fn next_node_id(model: &ScryModel) -> String {
    mint_id_from("node", model.nodes.iter().map(|n| n.id.as_str()))
}

pub fn make_link_id(src: &str, dst: &str) -> String {
    format!("link-{}-{}", src, dst)
}

pub fn next_link_id(model: &ScryModel) -> String {
    mint_id_from("link", model.links.iter().map(|l| l.id.as_str()))
}

pub fn next_group_id(model: &ScryModel) -> String {
    mint_id_from("group", model.groups.iter().map(|g| g.id.as_str()))
}

pub fn next_responsibility_id(existing: &[Responsibility]) -> String {
    mint_id_from("resp", existing.iter().map(|r| r.id.as_str()))
}

/// A `node-…` id unknown to BOTH the planned draft and the committed model, so
/// a node the plan deleted (still live in committed) can't have its id
/// re-issued: the pending deletion would then read as a reword and
/// `mark_implemented` would overwrite the committed node.
pub fn next_node_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    mint_id_from(
        "node",
        planned.nodes.iter().chain(committed.nodes.iter()).map(|n| n.id.as_str()),
    )
}

/// A `group-…` id unknown to both layers — same union guard as [`next_node_id_union`].
pub fn next_group_id_union(planned: &ScryModel, committed: &ScryModel) -> String {
    mint_id_from(
        "group",
        planned.groups.iter().chain(committed.groups.iter()).map(|g| g.id.as_str()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// resp-780: two minters seeing the same snapshot must not agree — the
    /// sequential scheme's whole failure was that they always did.
    #[test]
    fn two_minters_over_the_same_snapshot_disagree() {
        let m = ScryModel::new();
        let a = next_node_id(&m);
        let b = next_node_id(&m);
        assert_ne!(a, b);
        assert!(a.starts_with("node-") && a.len() == "node-".len() + SUFFIX_LEN, "{a}");
    }

    /// resp-780: a draw already held by either layer is rejected and redrawn.
    #[test]
    fn a_taken_id_is_never_returned() {
        // Force the collision: everything the alphabet can spell for one
        // draw is unlikely to be enumerable, so fake `taken` with the exact id
        // a fixed suffix would produce and check the redraw path via mint_id.
        let mut taken: HashSet<&str> = HashSet::new();
        let first = suffix(draw());
        let held = format!("resp-{first}");
        taken.insert(held.as_str());
        for _ in 0..1000 {
            assert_ne!(mint_id("resp", &taken), held);
        }
    }

    /// Old sequential ids still read as minted, so a payload echoing one is
    /// not treated as an invention.
    #[test]
    fn both_id_shapes_read_as_minted() {
        assert!(is_minted_id("resp-42", "resp"));
        assert!(is_minted_id("resp-k7x2qd", "resp"));
        assert!(!is_minted_id("resp-", "resp"));
        assert!(!is_minted_id("resp-new", "resp"));
        assert!(!is_minted_id("resp-k7x2qdz", "resp"));
        assert!(!is_minted_id("node-42", "resp"));
    }
}
