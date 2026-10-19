//! Defines the canonical model schema — nodes, links, groups, responsibilities, properties — that every other module reads and writes.
//!
//! Guards every model/plan write behind an exclusive lock and the fossilization (last-touched) clock, keeping the plan and committed layers reconciled as one working view.
//!
//! Resolves where a project's committed model, draft plan, and derived caches live on disk.

pub mod build_edges;
pub mod changes;
pub mod concerns;
pub mod derived_status;
pub mod derived_tree;
pub mod diff;
pub mod ears;
pub mod health;
pub mod ids;
pub mod landing;
pub mod model;
pub mod model_ref;
pub mod ownership;
pub mod rules;
pub mod seed;
pub mod style;
pub mod style_health;
pub mod test_results;
pub mod validate;
pub mod verdicts;
pub mod agent_settings;
pub mod drift_scope;
pub mod history_events;
pub mod product_code;
pub mod refusal;
pub mod session;
pub mod layers;
pub mod agent_setup;
