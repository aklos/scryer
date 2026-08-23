use std::collections::BTreeMap;
use std::path::Path;

/// File categories for annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Manifest,
    Infrastructure,
    Environment,
}

impl Category {
    fn label(self) -> &'static str {
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
fn classify_file(name: &str, rel_path: &Path) -> Option<Category> {
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
struct TreeNode {
    is_dir: bool,
    annotation: Option<&'static str>,
    children: BTreeMap<String, TreeNode>,
    has_annotated_descendant: bool,
}

impl TreeNode {
    fn new_dir() -> Self {
        Self {
            is_dir: true,
            annotation: None,
            children: BTreeMap::new(),
            has_annotated_descendant: false,
        }
    }

    fn new_file(annotation: Option<&'static str>) -> Self {
        Self {
            is_dir: false,
            annotation,
            children: BTreeMap::new(),
            has_annotated_descendant: false,
        }
    }

    /// Ensure a directory node exists at the given path components, creating intermediaries.
    fn ensure_dir(&mut self, components: &[&str]) -> &mut TreeNode {
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
    fn propagate_annotations(&mut self) -> bool {
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
    fn render(&self, out: &mut String, prefix: &str, depth: usize, max_context_depth: usize) {
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

/// Scan a project directory and return an annotated tree of architecturally relevant files.
/// Quick check: does the directory look like a codebase?
/// Looks for `.git`, manifest files, or common source directories at the root level.
pub fn is_codebase(path: &Path) -> bool {
    const MANIFEST_FILES: &[&str] = &[
        "package.json", "Cargo.toml", "go.mod", "pyproject.toml", "setup.py",
        "pom.xml", "build.gradle", "build.gradle.kts", "Gemfile",
        "composer.json", "mix.exs", "pubspec.yaml", "Package.swift",
        "Makefile", "CMakeLists.txt", "deno.json", "flake.nix",
    ];
    if path.join(".git").exists() {
        return true;
    }
    for name in MANIFEST_FILES {
        if path.join(name).exists() {
            return true;
        }
    }
    // Check for .csproj/.sln files
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".csproj") || name.ends_with(".fsproj") || name.ends_with(".sln") {
                return true;
            }
        }
    }
    false
}

/// Return relative paths of directories containing manifest files, excluding the
/// project root. Each entry is `(dir_relative_path, manifest_filename)`.
pub fn manifest_dirs(path: &Path) -> Vec<(String, String)> {
    let mut results: Vec<(String, String)> = Vec::new();

    let walker = ignore::WalkBuilder::new(path)
        .hidden(false)
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                if SKIP_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
                if SKIP_BUILD_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
            }
            true
        })
        .build();

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let rel = match entry.path().strip_prefix(path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let file_name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if classify_file(file_name, rel) == Some(Category::Manifest) {
            let dir = rel
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if !dir.is_empty() {
                results.push((dir, file_name.to_string()));
            }
        }
    }

    results.sort();
    results.dedup();
    results
}

pub fn project_structure(path: &Path) -> Result<String, String> {
    if !path.is_dir() {
        return Err(format!("'{}' is not a directory", path.display()));
    }

    let mut root = TreeNode::new_dir();

    let walker = ignore::WalkBuilder::new(path)
        .hidden(false) // show dotfiles like .github, .env.example
        .filter_entry(|entry| {
            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                let name = entry.file_name().to_string_lossy();
                // Skip noise directories
                if SKIP_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
                if SKIP_BUILD_DIRS.iter().any(|&s| name == s) {
                    return false;
                }
            }
            true
        })
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_path = entry.path();
        let rel = match entry_path.strip_prefix(path) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Skip root itself
        if rel.as_os_str().is_empty() {
            continue;
        }

        let components: Vec<&str> = rel
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or(""))
            .collect();

        if entry.file_type().is_some_and(|ft| ft.is_dir()) {
            root.ensure_dir(&components);
        } else if entry.file_type().is_some_and(|ft| ft.is_file()) {
            let file_name = components.last().copied().unwrap_or("");
            let annotation = classify_file(file_name, rel);

            // Ensure parent directories exist
            if components.len() > 1 {
                root.ensure_dir(&components[..components.len() - 1]);
            }

            let parent = if components.len() > 1 {
                root.ensure_dir(&components[..components.len() - 1])
            } else {
                &mut root
            };

            parent.children.insert(
                file_name.to_string(),
                TreeNode::new_file(annotation.map(|c| c.label())),
            );
        }
    }

    root.propagate_annotations();

    let mut output = String::from(".\n");
    // Depth 4 keeps real source structure visible (crate/src/module files)
    // while the walker's skip lists keep build output and vendored trees out.
    root.render(&mut output, "", 0, 4);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree must show the CODEBASE, not just its manifests: source files
    /// render (capped per directory), and structure stays visible several
    /// levels deep — the design-first flow starts from this tree.
    #[test]
    fn project_structure_shows_source_files_with_a_cap() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("api/src/handlers")).unwrap();
        std::fs::write(root.join("api/Cargo.toml"), "[package]\nname='api'").unwrap();
        std::fs::write(root.join("api/src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("api/src/handlers/auth.rs"), "").unwrap();
        // A directory over the per-dir file cap collapses its tail.
        std::fs::create_dir_all(root.join("api/generated")).unwrap();
        for i in 0..30 {
            std::fs::write(root.join(format!("api/generated/f{i:02}.rs")), "").unwrap();
        }

        let tree = project_structure(root).unwrap();
        assert!(tree.contains("main.rs"), "source files render: {tree}");
        assert!(tree.contains("auth.rs"), "nested source structure renders: {tree}");
        assert!(tree.contains("Cargo.toml"), "{tree}");
        assert!(
            tree.contains("f00.rs") && !tree.contains("f29.rs"),
            "per-dir cap holds: {tree}"
        );
        assert!(tree.contains("(5 more)"), "the collapsed tail is counted: {tree}");
    }

    #[test]
    fn classify_known_files() {
        assert!(matches!(
            classify_file("package.json", Path::new("package.json")),
            Some(Category::Manifest)
        ));
        assert!(matches!(
            classify_file("Cargo.toml", Path::new("Cargo.toml")),
            Some(Category::Manifest)
        ));
        assert!(matches!(
            classify_file("Dockerfile", Path::new("Dockerfile")),
            Some(Category::Infrastructure)
        ));
        assert!(matches!(
            classify_file("Dockerfile.builder", Path::new("Dockerfile.builder")),
            Some(Category::Infrastructure)
        ));
        assert!(matches!(
            classify_file("fly.toml", Path::new("fly.toml")),
            Some(Category::Infrastructure)
        ));
        assert!(matches!(
            classify_file(".env.example", Path::new(".env.example")),
            Some(Category::Environment)
        ));
        assert!(classify_file("README.md", Path::new("README.md")).is_none());
        assert!(classify_file("index.ts", Path::new("src/index.ts")).is_none());
    }

    #[test]
    fn classify_ci_files() {
        assert!(matches!(
            classify_file(
                "deploy.yml",
                Path::new(".github/workflows/deploy.yml")
            ),
            Some(Category::Infrastructure)
        ));
        assert!(matches!(
            classify_file(".gitlab-ci.yml", Path::new(".gitlab-ci.yml")),
            Some(Category::Infrastructure)
        ));
    }

    #[test]
    fn classify_terraform() {
        assert!(matches!(
            classify_file("main.tf", Path::new("infra/main.tf")),
            Some(Category::Infrastructure)
        ));
    }

    #[test]
    fn product_code_is_parseable_source_only() {
        assert!(is_product_code("src/App.tsx"));
        assert!(is_product_code("crates/scryer-core/src/lib.rs"));
        // Non-source: assets, lockfiles, manifests, docs.
        assert!(!is_product_code("demo/repo-qr.png"));
        assert!(!is_product_code("pnpm-lock.yaml"));
        assert!(!is_product_code("package.json"));
        assert!(!is_product_code("README.md"));
        // Source-shaped but carries no architecture.
        assert!(!is_product_code("src/types/api.d.ts"));
        assert!(!is_product_code("docs/src/stubs/tauri.ts"));
        assert!(!is_product_code("src/schema.generated.ts"));
        assert!(!is_product_code("app/__generated__/gql.ts"));
    }

    /// A directory counts as a codebase when it has a `.git` folder or a
    /// manifest file at the root (including .csproj/.sln discovered by
    /// extension); a bare directory does not.
    #[test]
    fn a_git_folder_or_manifest_marks_a_codebase() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_codebase(dir.path()), "an empty directory is not a codebase");

        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(is_codebase(dir.path()));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert!(is_codebase(dir.path()));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("App.csproj"), "").unwrap();
        assert!(is_codebase(dir.path()));
    }
}
