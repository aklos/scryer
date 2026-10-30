//! The C4 model itself: its nodes, links and claims, the concern vocabulary,
//! and where a project's scryer files live. [domain]

pub mod model;
pub mod concerns;
pub mod model_ref;

pub use model::*;
pub use concerns::*;
pub use model_ref::*;
