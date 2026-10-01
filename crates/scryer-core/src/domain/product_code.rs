//! Which files count as product code, and the annotated tree a reader sees.
//! Deciding it is a question about a path, not about the disk — walking the
//! project is `infrastructure::scan`.

use std::collections::BTreeMap;
use std::path::Path;

/// File categories for annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Category {
    Manifest,
    Infrastructure,
    Environment,
}

impl Category {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Category::Manifest => "manifest",
            Category::Infrastructure => "infrastructure",
            Category::Environment => "environment",
        }
    }
}

/// Directories to skip even if not in .gitignore.
pub const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".scryer",
    ".next",
    "__pycache__",
    ".direnv",
    ".venv",
    ".turbo",
    ".cache",
    ".nuxt",
    ".output",
    ".svelte-kit",
    ".parcel-cache",
    ".webpack",
    "vendor", // Go, Ruby, PHP
];

/// Directories that are build output and uninteresting for structure.
pub const SKIP_BUILD_DIRS: &[&str] = &[
    "dist",
    "build",
    "out",
    "target",
    ".build",
    "bin",
    "obj", // .NET
    "pkg", // wasm-pack
];

/// Extensions of parseable source files — one per bundled tree-sitter grammar.
/// Kept in lockstep with `language_for_ext` in scryer-extract's `lang.rs`
/// (a test there asserts every entry here maps to a grammar).
pub const SOURCE_EXTS: &[&str] = &[
    "rs", // Rust
    "ts", "mts", "cts", "tsx", // TypeScript
    "js", "jsx", "mjs", "cjs", // JavaScript
    "vue", // Vue single-file components (script parsed as TypeScript)
    "py", "pyi", // Python
    "go", // Go
    "java", // Java
    "rb", // Ruby
    "c", "h", // C
    "cpp", "cc", "cxx", "hpp", "hh", "hxx", // C++
    "cs", // C#
    "php", // PHP
];

/// Is this project-relative path product source code — a file whose change can
/// carry architecture? True only for parseable source extensions, excluding
/// code that exists but carries none: TypeScript declaration/mirror files
/// (`*.d.ts`), test doubles in a `stubs/` directory, and generated sources.
/// The single gate shared by extraction (which files mint symbols) and drift
/// (which changed files demand reconciliation) — assets, lockfiles, and
/// generated churn never reach a modeling agent through either path.
pub fn is_product_code(rel_path: &str) -> bool {
    let ext = rel_path.rsplit('.').next().unwrap_or_default();
    if !SOURCE_EXTS.contains(&ext) {
        return false;
    }
    if rel_path.ends_with(".d.ts") {
        return false;
    }
    let mut segs = rel_path.split('/');
    if segs.any(|s| s == "stubs" || s == "generated" || s == "__generated__") {
        return false;
    }
    let file = rel_path.rsplit('/').next().unwrap_or(rel_path);
    if file.contains(".generated.") || file.contains(".gen.") {
        return false;
    }
    true
}

/// Classify a file by its name (not full path).
pub(crate) fn classify_file(name: &str, rel_path: &Path) -> Option<Category> {
    // Manifests
    match name {
        "package.json" | "Cargo.toml" | "go.mod" | "pyproject.toml" | "setup.py"
        | "setup.cfg" | "pom.xml" | "build.gradle" | "build.gradle.kts" | "Gemfile"
        | "composer.json" | "mix.exs" | "pubspec.yaml" | "Package.swift"
        | "Makefile" | "CMakeLists.txt" | "deno.json" | "deno.jsonc"
        | "bun.lock" | "flake.nix" => return Some(Category::Manifest),
        _ => {}
    }
    if name.ends_with(".csproj") || name.ends_with(".fsproj") || name.ends_with(".sln") {
        return Some(Category::Manifest);
    }

    // Infrastructure
    match name {
        "fly.toml" | "Procfile" | "vercel.json" | "netlify.toml" | "render.yaml"
        | "railway.json" | "app.yaml" | "Jenkinsfile" | "shell.nix"
        | "docker-compose.yml" | "docker-compose.yaml"
        | "serverless.yml" | "serverless.yaml" | "skaffold.yaml" => {
            return Some(Category::Infrastructure)
        }
        _ => {}
    }
    if name.starts_with("Dockerfile") {
        return Some(Category::Infrastructure);
    }
    if name.starts_with("docker-compose") && (name.ends_with(".yml") || name.ends_with(".yaml")) {
        return Some(Category::Infrastructure);
    }
    if name.ends_with(".tf") || name.ends_with(".tfvars") {
        return Some(Category::Infrastructure);
    }
    // SAM / CloudFormation templates
    if name == "template.yaml"
        || name == "template.yml"
        || name == "sam.yaml"
        || name == "sam.yml"
        || name == "deploy.yml"
        || name == "deploy.yaml"
    {
        return Some(Category::Infrastructure);
    }
    // CI/CD — normalized so the `/`-separated prefixes match on Windows too.
    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
    if rel_str.starts_with(".github/workflows/") && (name.ends_with(".yml") || name.ends_with(".yaml"))
    {
        return Some(Category::Infrastructure);
    }
    if name == "config.yml" && rel_str.starts_with(".circleci/") {
        return Some(Category::Infrastructure);
    }
    if name == ".gitlab-ci.yml" {
        return Some(Category::Infrastructure);
    }
    // K8s manifests in conventional directories
    if (rel_str.starts_with("k8s/") || rel_str.starts_with("kubernetes/") || rel_str.starts_with("deploy/") || rel_str.starts_with("infra/"))
        && (name.ends_with(".yml") || name.ends_with(".yaml"))
    {
        return Some(Category::Infrastructure);
    }

    // Environment
    if name == ".env.example" || name == ".env.sample" || name == ".env.template" {
        return Some(Category::Environment);
    }

    None
}

/// A node in the scanned tree.
pub(crate) struct TreeNode {
    is_dir: bool,
    annotation: Option<&'static str>,
    pub(crate) children: BTreeMap<String, TreeNode>,
    has_annotated_descendant: bool,
}

impl TreeNode {
    pub(crate) fn new_dir() -> Self {
        Self {
            is_dir: true,
            annotation: None,
            children: BTreeMap::new(),
            has_annotated_descendant: false,
        }
    }

    pub(crate) fn new_file(annotation: Option<&'static str>) -> Self {
        Self {
            is_dir: false,
            annotation,
            children: BTreeMap::new(),
            has_annotated_descendant: false,
        }
    }

    /// Ensure a directory node exists at the given path components, creating intermediaries.
    pub(crate) fn ensure_dir(&mut self, components: &[&str]) -> &mut TreeNode {
        let mut current = self;
        for &comp in components {
            current = current
                .children
                .entry(comp.to_string())
                .or_insert_with(TreeNode::new_dir);
        }
        current
    }

    /// Propagate `has_annotated_descendant` bottom-up.
    pub(crate) fn propagate_annotations(&mut self) -> bool {
        if !self.is_dir {
            return self.annotation.is_some();
        }
        let mut any = false;
        for child in self.children.values_mut() {
            if child.propagate_annotations() {
                any = true;
            }
        }
        self.has_annotated_descendant = any;
        any
    }

    /// Render this tree as annotated text.
    pub(crate) fn render(&self, out: &mut String, prefix: &str, depth: usize, max_context_depth: usize) {
        // Files-per-directory cap: the tree must SHOW the codebase (the
        // design-first flow starts from it), but a generated or vendored
        // directory with hundreds of files should not drown the shape.
        const FILE_CAP: usize = 25;

        // Separate children into categories
        let mut annotated_files: Vec<(&str, &str)> = Vec::new();
        let mut plain_files: Vec<&str> = Vec::new();
        let mut interesting_dirs: Vec<(&str, &TreeNode)> = Vec::new();
        let mut context_dirs: Vec<(&str, &TreeNode)> = Vec::new();
        let mut hidden_count: usize = 0;

        for (name, child) in &self.children {
            if child.is_dir {
                if child.has_annotated_descendant {
                    interesting_dirs.push((name.as_str(), child));
                } else if !child.children.is_empty() && depth < max_context_depth {
                    context_dirs.push((name.as_str(), child));
                } else if !child.children.is_empty() {
                    hidden_count += 1;
                }
            } else if let Some(label) = child.annotation {
                annotated_files.push((name.as_str(), label));
            } else {
                plain_files.push(name.as_str());
            }
        }
        let shown_plain = plain_files.len().min(FILE_CAP);
        hidden_count += plain_files.len() - shown_plain;

        let total_items = annotated_files.len()
            + shown_plain
            + interesting_dirs.len()
            + context_dirs.len()
            + if hidden_count > 0 { 1 } else { 0 };
        let mut idx = 0;

        // Annotated files first
        for (name, label) in &annotated_files {
            idx += 1;
            let connector = if idx == total_items { "└── " } else { "├── " };
            let padding = 30usize.saturating_sub(name.len());
            out.push_str(&format!(
                "{}{}{}{} [{}]\n",
                prefix, connector, name,
                " ".repeat(padding),
                label
            ));
        }

        // Then the plain source files — the codebase itself, not just its
        // manifests.
        for name in plain_files.iter().take(shown_plain) {
            idx += 1;
            let connector = if idx == total_items { "└── " } else { "├── " };
            out.push_str(&format!("{}{}{}\n", prefix, connector, name));
        }

        // Interesting dirs (have annotated descendants) — recurse
        for (name, child) in &interesting_dirs {
            idx += 1;
            let connector = if idx == total_items { "└── " } else { "├── " };
            let extension = if idx == total_items { "    " } else { "│   " };
            out.push_str(&format!("{}{}{}/\n", prefix, connector, name));
            let child_prefix = format!("{}{}", prefix, extension);
            child.render(out, &child_prefix, depth + 1, max_context_depth);
        }

        // Context dirs (no annotations, just structure) — recurse to show shape
        for (name, child) in &context_dirs {
            idx += 1;
            let connector = if idx == total_items { "└── " } else { "├── " };
            let extension = if idx == total_items { "    " } else { "│   " };
            out.push_str(&format!("{}{}{}/\n", prefix, connector, name));
            let child_prefix = format!("{}{}", prefix, extension);
            child.render(out, &child_prefix, depth + 1, max_context_depth);
        }

        // Hidden content (unannotated files or dirs beyond depth limit)
        if hidden_count > 0 {
            idx += 1;
            let connector = if idx == total_items { "└── " } else { "├── " };
            out.push_str(&format!("{}{}... ({} more)\n", prefix, connector, hidden_count));
        }
    }
}
