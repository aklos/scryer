//! The C4 model itself: its nodes, links and claims, the ids that name them, the
//! concern vocabulary, the structural seed, and where a project's scryer files
//! live. [domain]

pub mod model;
pub mod ids;
pub mod concerns;
pub mod seed;
pub mod model_ref;

pub use model::*;
pub use ids::*;
pub use concerns::*;
pub use seed::*;
pub use model_ref::*;
