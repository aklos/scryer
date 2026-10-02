//! The facts a project's manifests and configs declare — the vocabulary the
//! dependency graph is built in. Reading them off disk is the infrastructure's
//! job; these are just the shapes it produces.

/// A declared build/deploy unit.
#[derive(Debug, Clone)]
pub struct Container {
    /// Directory relative to the project root, normalized with `/` separators.
    /// Empty string for the project root.
    pub dir: String,
    /// Declared name (crate/package name) or the directory basename.
    pub name: String,
    /// Literal declared technology — currently a Dockerfile base image. `None`
    /// when nothing is declared (Pass 2 names what the unit is).
    pub technology: Option<String>,
    /// Directories of other containers this one declares a path dependency on.
    pub dep_dirs: Vec<String>,
    /// The declared go.mod module path (`module github.com/acme/proj`) — the
    /// prefix Go import specs spell to reach this container's packages.
    pub go_module: Option<String>,
    /// Source roots this unit declares for Clojure namespace resolution —
    /// deps.edn / bb.edn `:paths`, shadow-cljs.edn / project.clj
    /// `:source-paths`. Relative to `dir`. A Clojure namespace maps to a file
    /// path under one of these, so without them `app.db` is unresolvable.
    pub clj_paths: Vec<String>,
}

/// The flattened alias table governing one directory subtree.
#[derive(Debug, Clone, Default)]
pub struct TsAliases {
    /// Directory holding the config (project-relative, `""` for the root).
    /// Files under it resolve bare specs through this table; the NEAREST
    /// (longest-dir) table wins when configs nest.
    pub dir: String,
    /// `baseUrl` resolved project-relative: a bare spec may denote a file
    /// under it.
    pub base_url: Option<String>,
    /// `paths` pattern -> substitution targets, targets already resolved
    /// project-relative with their `*` retained (`@/*` -> `["src/*"]`).
    pub paths: Vec<(String, Vec<String>)>,
}
