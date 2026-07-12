//! Lockstep fixtures for the frontend's ported diff engine.
//!
//! `src/planDiff.ts` re-implements `diff.rs` in TypeScript because the canvas
//! computes the plan diff client-side. The two must never disagree: this test
//! pins diff.rs's behaviour to JSON fixtures under `tests/lockstep/`, and a
//! vitest suite (`src/planDiff.lockstep.test.ts`) asserts planDiff.ts computes
//! the identical diff from the same inputs.
//!
//! When a diff.rs change is intentional, regenerate the fixtures and re-run
//! the frontend test:
//!
//! ```sh
//! UPDATE_LOCKSTEP=1 cargo test -p scryer-core --test lockstep
//! pnpm test
//! ```

use scryer_core::{diff::diff, ScryModel};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lockstep")
}

fn model(v: Value) -> ScryModel {
    // Route through the real deserializer so fixtures can only contain shapes
    // the product accepts.
    serde_json::from_value(v).expect("fixture model must deserialize as ScryModel")
}

/// The committed base every scenario diffs against: two nodes with claims,
/// a data-shape symbol, a link, and a group.
fn base() -> ScryModel {
    model(json!({
        "version": "0.3",
        "nodes": [
            { "id": "node-1", "kind": "system", "name": "Shop" },
            {
                "id": "node-2", "kind": "container", "name": "API",
                "parentId": "node-1", "technology": "Rust",
                "description": "The service", "directives": ["never block"],
                "responsibilities": [
                    { "id": "resp-1", "statement": "authenticates users" },
                    { "id": "resp-2", "statement": "serves the catalog",
                      "directives": ["cache reads"] }
                ]
            },
            {
                "id": "node-3", "kind": "container", "name": "Worker",
                "parentId": "node-1",
                "responsibilities": [
                    { "id": "resp-3", "statement": "sends receipts" }
                ]
            },
            {
                "id": "node-4", "kind": "symbol", "name": "Order",
                "parentId": "node-2",
                "properties": [
                    { "label": "id", "description": "order id" },
                    { "label": "total", "description": "grand total" }
                ]
            }
        ],
        "links": [
            { "id": "link-1", "src": "node-2", "dst": "node-3", "label": "enqueues", "method": "queue" }
        ],
        "groups": [
            {
                "id": "group-1", "name": "Backend", "memberIds": ["node-2", "node-3"],
                "parentNodeId": "node-1",
                "responsibilities": [ { "id": "resp-4", "statement": "deploys atomically" } ]
            }
        ]
    }))
}

/// Every change category at once: node add/delete/move + every reworded field,
/// link add/delete/repoint/relabel, responsibility add/delete/move/reword,
/// property add/delete/reword, group rename/members-changed.
fn scenario_everything() -> (ScryModel, ScryModel) {
    let from = base();
    let to = model(json!({
        "version": "0.3",
        "nodes": [
            { "id": "node-1", "kind": "system", "name": "Shop" },
            {
                // Reworded on every truth-bearing field, plus moved (reparented
                // under node-5) and a kind change.
                "id": "node-2", "kind": "component", "name": "Public API",
                "parentId": "node-5", "technology": "Rust 2024",
                "description": "The edge service", "directives": ["never block", "log rejects"],
                "external": true,
                "responsibilities": [
                    // resp-1 reworded (statement + directives)
                    { "id": "resp-1", "statement": "authenticates and authorizes users",
                      "directives": ["use the session store"] },
                    // resp-3 moved here from node-3
                    { "id": "resp-3", "statement": "sends receipts" },
                    // resp-5 added
                    { "id": "resp-5", "statement": "rate-limits clients" }
                ]
            },
            // node-3 deleted; node-5 added
            { "id": "node-5", "kind": "container", "name": "Gateway", "parentId": "node-1" },
            {
                "id": "node-4", "kind": "symbol", "name": "Order",
                "parentId": "node-2",
                "properties": [
                    // "id" unchanged, "total" reworded, "currency" added,
                    // (committed "total" description changes; nothing deleted here)
                    { "label": "id", "description": "order id" },
                    { "label": "total", "description": "grand total, in cents" },
                    { "label": "currency", "description": "ISO 4217" }
                ]
            }
        ],
        "links": [
            // link-1 repointed (dst) and relabeled; link-2 added
            { "id": "link-1", "src": "node-2", "dst": "node-5", "label": "routes via", "method": "http" },
            { "id": "link-2", "src": "node-5", "dst": "node-2", "label": "forwards to" }
        ],
        "groups": [
            {
                // renamed + membership changed (node-3 out, node-5 in);
                // resp-2 relocated here from node-2 (a claim moved node → group)
                "id": "group-1", "name": "Edge", "memberIds": ["node-2", "node-5"],
                "parentNodeId": "node-1",
                "responsibilities": [
                    { "id": "resp-4", "statement": "deploys atomically" },
                    { "id": "resp-2", "statement": "serves the catalog",
                      "directives": ["cache reads"] }
                ]
            }
        ]
    }));
    (from, to)
}

/// Identical layers — the diff must be empty (a fresh project's plan).
fn scenario_identical() -> (ScryModel, ScryModel) {
    (base(), base())
}

/// Design-first: nothing committed yet, so every element reads `added`.
fn scenario_greenfield() -> (ScryModel, ScryModel) {
    (
        model(json!({ "version": "0.3", "nodes": [], "links": [], "groups": [] })),
        base(),
    )
}

/// Property deletion is keyed by `(owner, label)`, so a relabel must read as
/// delete-plus-add — pinned separately because it's the one identity rule that
/// differs from the id-keyed elements.
fn scenario_property_relabel() -> (ScryModel, ScryModel) {
    let from = base();
    let mut v = serde_json::to_value(base()).unwrap();
    v["nodes"][3]["properties"][1]["label"] = json!("grand_total");
    (from, model(v))
}

#[test]
fn lockstep_fixtures_are_current() {
    let scenarios: Vec<(&str, (ScryModel, ScryModel))> = vec![
        ("everything", scenario_everything()),
        ("identical", scenario_identical()),
        ("greenfield", scenario_greenfield()),
        ("property-relabel", scenario_property_relabel()),
    ];

    let dir = fixture_dir();
    let update = std::env::var("UPDATE_LOCKSTEP").is_ok();
    if update {
        fs::create_dir_all(&dir).unwrap();
    }

    for (name, (from, to)) in scenarios {
        let fixture = json!({
            "from": from,
            "to": to,
            "diff": diff(&from, &to),
        });
        let path = dir.join(format!("{name}.json"));
        let rendered = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
        if update {
            fs::write(&path, &rendered).unwrap();
        } else {
            let on_disk = fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!("missing fixture {path:?} — run UPDATE_LOCKSTEP=1 cargo test -p scryer-core --test lockstep")
            });
            assert_eq!(
                on_disk, rendered,
                "diff.rs no longer matches tests/lockstep/{name}.json.\n\
                 If the change is intentional: UPDATE_LOCKSTEP=1 cargo test -p scryer-core --test lockstep, \
                 then `pnpm test` to prove planDiff.ts still agrees."
            );
        }
    }
}
