use std::collections::BTreeMap;
use std::path::Path;

/// File categories for annotation.
#[derive(Debug, Clone, Copy)]
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
    // CI/CD
    let rel_str = rel_path.to_string_lossy();
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
        // Separate children into categories
        let mut annotated_files: Vec<(&str, &str)> = Vec::new();
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
                hidden_count += 1;
            }
        }

        let total_items = annotated_files.len()
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
    root.render(&mut output, "", 0, 1);

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
