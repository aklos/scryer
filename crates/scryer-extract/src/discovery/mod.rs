//! Reading the project off disk: its files, its containers from their manifests,
//! and its TypeScript path aliases. [infrastructure]

pub mod walk;
pub mod manifest;
pub mod tsconfig;

pub use walk::*;
pub use manifest::*;
pub use tsconfig::*;
