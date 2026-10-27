//! Architectural styles and how the code keeps them: the style table, the
//! build's dependency graph, and the violations its imports show. [domain]

pub mod style;
pub mod style_health;
pub mod dependency_graph;

pub use style::*;
pub use style_health::*;
pub use dependency_graph::*;
