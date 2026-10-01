use std::path::Path;

pub use crate::domain::product_code::*;

/// Scan a project directory and return an annotated tree of architecturally relevant files.
/// Quick check: does the directory look like a codebase?
/// Looks for `.git`, manifest files, or common source directories at the root level.
pub fn dir_is_codebase(path: &Path) -> bool {
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
pub fn find_manifest_dirs(path: &Path) -> Vec<(String, String)> {
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

pub fn render_project_tree(path: &Path) -> Result<String, String> {
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

        let tree = render_project_tree(root).unwrap();
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
        assert!(!dir_is_codebase(dir.path()), "an empty directory is not a codebase");

        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(dir_is_codebase(dir.path()));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert!(dir_is_codebase(dir.path()));

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("App.csproj"), "").unwrap();
        assert!(dir_is_codebase(dir.path()));
    }
}
