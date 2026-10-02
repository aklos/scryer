//! The extraction entry points another container calls: walk the project,
//! read what its manifests declare, parse its sources, and hand back the map.

use crate::domain::context::{self, ProjectContext};
use crate::infrastructure::{manifest, tsconfig, walk};
use std::path::Path;

pub use crate::infrastructure::walk::ParseStats as ExtractionStats;

/// Walk a project directory and build its deterministic [`ProjectContext`].
pub fn extract_context(project: &Path) -> Result<ProjectContext, String> {
    extract_context_with_stats(project).map(|(context, _)| context)
}

/// Every file under `project` (project-relative, forward-slashed), skipping the
/// same vendor/build directories the extractor ignores. A cheap walk with no
/// parse — used to resolve boundary globs against real files (e.g. for
/// completeness: does a node's claimed territory actually contain code?).
pub fn list_project_files(project: &Path) -> std::collections::BTreeSet<String> {
    walk::project_files(project)
}

/// Extract with instrumentation for build logs and performance regression
/// checks. Unchanged source files reuse their previous parser output.
pub fn extract_context_with_stats(
    project: &Path,
) -> Result<(ProjectContext, ExtractionStats), String> {
    if !project.is_dir() {
        return Err(format!("'{}' is not a directory", project.display()));
    }
    let all_files = walk::walk_files(project);
    let containers = manifest::discover_containers_from_files(project, &all_files);
    let ts_aliases = tsconfig::discover_ts_aliases(project, &all_files);
    let (files, stats) = walk::parse_sources(project, &all_files);

    let project_name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let context = context::build_context(&project_name, &containers, &files, &ts_aliases);
    Ok((context, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::lang;
    use crate::{compact_scope, slice_container, slice_scope};
    use std::collections::HashSet;

    /// The product-code gate lives in scryer-core (shared with drift scoping);
    /// its exclusion semantics are extraction's contract, so pin them here.
    /// NOTE: config files are deliberately NOT excluded — a CMS/ORM collection
    /// config (e.g. Payload, Drizzle) declares the real data model, so whether
    /// a config earns a symbol stays the agent's judgment.
    #[test]
    fn excludes_non_product_files() {
        use scryer_core::scan::is_product_code;
        assert!(!is_product_code("docs/src/stubs/tauri.ts"));
        assert!(!is_product_code("src/types/api.d.ts"));
        assert!(!is_product_code("src/schema.generated.ts"));
        assert!(!is_product_code("app/__generated__/gql.ts"));
        // real product code stays
        assert!(is_product_code("crates/scryer-extract/src/manifest.rs"));
        assert!(is_product_code("src/App.tsx"));
        // config is NOT excluded — may declare a real data model
        assert!(is_product_code("docs/src/content.config.ts"));
        assert!(is_product_code("src/collections/Users.ts"));
    }

    /// `scan::SOURCE_EXTS` mirrors the parser registry: every extension the
    /// shared product-code gate accepts must actually parse here.
    #[test]
    fn source_exts_stay_in_lockstep_with_grammars() {
        for ext in scryer_core::scan::SOURCE_EXTS {
            assert!(
                lang::supports_ext(ext),
                "scan::SOURCE_EXTS lists '{ext}' but lang.rs cannot parse it"
            );
        }
    }

    /// Ad-hoc: dump the container/file/symbol/edge summary for an arbitrary repo.
    /// `REPO=/path cargo test -p scryer-extract dump_external -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_external() {
        let repo = std::path::PathBuf::from(std::env::var("REPO").expect("set REPO"));
        let ctx = extract_context(&repo).expect("extraction");
        eprintln!("=== {} containers ===", ctx.containers.len());
        for c in &ctx.containers {
            let files = ctx
                .files
                .iter()
                .filter(|f| f.container_dir == c.dir)
                .count();
            let syms: usize = ctx
                .files
                .iter()
                .filter(|f| f.container_dir == c.dir)
                .map(|f| f.symbols.len())
                .sum();
            eprintln!(
                "  '{}' (dir='{}')  tech={:?}  files={}  symbols={}",
                c.name, c.dir, c.technology, files, syms
            );
        }
        let syms: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        eprintln!(
            "totals: {} files, {} symbols, {} symbol-edges, {} file-edges",
            ctx.files.len(),
            syms,
            ctx.symbol_edges.len(),
            ctx.file_edges.len()
        );
    }

    /// Run extraction on this very repository and assert the context holds
    /// together: a project name, the workspace crates as containers, plenty of
    /// symbols, unique source-anchored keys, and edges that reference only real
    /// keys/files. No `ScryModel` is built — this layer emits a map, not a model.
    #[test]
    fn extracts_this_repo_cleanly() {
        // crates/scryer-extract -> repo root
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let ctx = extract_context(&repo).expect("extraction");

        assert!(!ctx.project_name.is_empty(), "has a project name");
        assert!(
            ctx.containers.len() >= 5,
            "the workspace crates as containers (got {})",
            ctx.containers.len()
        );
        let symbols: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        assert!(
            symbols > 100,
            "many symbols from a repo this size (got {symbols})"
        );

        eprintln!(
            "context: {} containers, {} files, {} symbols, {} symbol-edges, {} file-edges",
            ctx.containers.len(),
            ctx.files.len(),
            symbols,
            ctx.symbol_edges.len(),
            ctx.file_edges.len()
        );
        let (full_bytes, compact_bytes) = ctx.containers.iter().fold(
            (0usize, 0usize),
            |(full_total, compact_total), container| {
                let scope = slice_container(&ctx, &container.dir);
                let compact = compact_scope(&scope);
                eprintln!(
                    "  scope '{}': {} files, {} work units, {} bytes",
                    container.dir,
                    scope.files.len(),
                    compact.work_units(),
                    serde_json::to_vec(&compact).unwrap().len(),
                );
                (
                    full_total + serde_json::to_vec(&scope).unwrap().len(),
                    compact_total + serde_json::to_vec(&compact).unwrap().len(),
                )
            },
        );
        eprintln!(
            "prompt payload: {} -> {} bytes ({:.1}% smaller)",
            full_bytes,
            compact_bytes,
            100.0 - (compact_bytes as f64 / full_bytes.max(1) as f64 * 100.0),
        );

        // Files are emitted in rel_path order (determinism guarantee).
        let mut sorted = ctx.files.clone();
        sorted.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        assert!(
            ctx.files
                .iter()
                .map(|f| &f.rel_path)
                .eq(sorted.iter().map(|f| &f.rel_path)),
            "files sorted by rel_path"
        );

        // Every symbol key is unique and source-anchored (never a node-N id).
        let keys: HashSet<&str> = ctx
            .files
            .iter()
            .flat_map(|f| f.symbols.iter().map(|s| s.key.as_str()))
            .collect();
        let key_count: usize = ctx.files.iter().map(|f| f.symbols.len()).sum();
        assert_eq!(keys.len(), key_count, "symbol keys are unique");
        assert!(
            keys.iter().all(|k| k.contains('#') && k.contains('@')),
            "keys are rel_path#name@line, not node ids"
        );

        // Every container_dir on a file resolves to a real container.
        let cdirs: HashSet<&str> = ctx.containers.iter().map(|c| c.dir.as_str()).collect();
        for f in &ctx.files {
            assert!(
                cdirs.contains(f.container_dir.as_str()),
                "file '{}' has unknown container '{}'",
                f.rel_path,
                f.container_dir
            );
        }

        // Edges reference only real keys / files (no dangling endpoints).
        for e in &ctx.symbol_edges {
            assert!(
                keys.contains(e.src.as_str()),
                "symbol edge src exists: {}",
                e.src
            );
            assert!(
                keys.contains(e.dst.as_str()),
                "symbol edge dst exists: {}",
                e.dst
            );
        }
        let rels: HashSet<&str> = ctx.files.iter().map(|f| f.rel_path.as_str()).collect();
        for e in &ctx.file_edges {
            assert!(
                rels.contains(e.src.as_str()),
                "file edge src exists: {}",
                e.src
            );
            assert!(
                rels.contains(e.dst.as_str()),
                "file edge dst exists: {}",
                e.dst
            );
        }

        // Slicing one crate yields a strict subset that still references real keys.
        let scope = "crates/scryer-extract";
        let scoped = slice_scope(&ctx, scope);
        assert!(!scoped.files.is_empty(), "the extract crate has files");
        assert!(
            scoped.files.iter().all(|f| f.rel_path.starts_with(scope)),
            "sliced files are all under the scope"
        );
        assert!(
            scoped.containers.iter().any(|c| c.dir == scope),
            "the scope's own container is present"
        );
    }

    /// End-to-end on a scratch pnpm-workspace repo: tsconfig alias imports
    /// (through real JSONC + `extends`), package-name imports across
    /// containers, and relative imports must all produce edges from a plain
    /// `extract_context` walk — the full wiring, no hand-built parses.
    #[test]
    fn extracts_ts_monorepo_import_edges() {
        let dir = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("package.json", r#"{"name":"monorepo"}"#);
        write("pnpm-workspace.yaml", "packages:\n  - packages/*\n");
        write(
            "tsconfig.base.json",
            r#"{
  // workspace-wide aliases, Nx-style, with a trailing comma
  "compilerOptions": { "paths": { "~shared/*": ["./packages/shared/src/*"], } },
}"#,
        );
        write("packages/ui/package.json", r#"{"name":"@acme/ui"}"#);
        write(
            "packages/ui/src/Button.tsx",
            "export function Button() { return <button/>; }\n",
        );
        write("packages/shared/package.json", r#"{"name":"@acme/shared"}"#);
        write(
            "packages/shared/src/dates.ts",
            "export function fmtDate(d: Date) { return String(d); }\n",
        );
        write("packages/app/package.json", r#"{"name":"@acme/app"}"#);
        write(
            "packages/app/tsconfig.json",
            r#"{ "extends": "../../tsconfig.base.json" }"#,
        );
        write(
            "packages/app/src/helpers.ts",
            "export function helper() { return 1; }\n",
        );
        write(
            "packages/app/src/App.tsx",
            r#"import { Button } from "@acme/ui";
import { fmtDate } from "~shared/dates";
import { helper } from "./helpers";
export function App() {
  helper();
  return <Button title={fmtDate(new Date())} />;
}
"#,
        );

        let ctx = extract_context(dir.path()).expect("extraction");
        let has_file_edge = |src: &str, dst: &str| {
            ctx.file_edges.iter().any(|e| e.src == src && e.dst == dst)
        };
        // package-name import across containers
        assert!(
            has_file_edge("packages/app/src/App.tsx", "packages/ui/src/Button.tsx"),
            "package import edge missing; edges: {:?}",
            ctx.file_edges
        );
        // tsconfig alias through extends + JSONC
        assert!(
            has_file_edge("packages/app/src/App.tsx", "packages/shared/src/dates.ts"),
            "alias import edge missing; edges: {:?}",
            ctx.file_edges
        );
        // relative import
        assert!(
            has_file_edge("packages/app/src/App.tsx", "packages/app/src/helpers.ts"),
            "relative import edge missing; edges: {:?}",
            ctx.file_edges
        );
        // usage-site symbol edges: App -> Button / fmtDate / helper
        let key_of = |name: &str| {
            ctx.files
                .iter()
                .flat_map(|f| &f.symbols)
                .find(|s| s.name == name)
                .map(|s| s.key.clone())
                .unwrap()
        };
        let app = key_of("App");
        for dst in ["Button", "fmtDate", "helper"] {
            let dst = key_of(dst);
            assert!(
                ctx.symbol_edges.iter().any(|e| e.src == app && e.dst == dst),
                "symbol edge App -> {dst} missing; edges: {:?}",
                ctx.symbol_edges
            );
        }
    }

    /// End-to-end on a scratch uv-style Python workspace: cross-package
    /// imports resolve through declared pyproject names (hyphens -> import
    /// underscores), relative imports within the package, symbol edges at
    /// usage sites — from a plain `extract_context` walk.
    #[test]
    fn extracts_py_monorepo_import_edges() {
        let dir = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "packages/lib/pyproject.toml",
            "[project]\nname = \"acme-lib\"\nversion = \"1.0\"\n",
        );
        write(
            "packages/lib/src/acme_lib/dates.py",
            "def fmt_date(d):\n    return str(d)\n",
        );
        write(
            "packages/app/pyproject.toml",
            "[project]\nname = \"acme-app\"\nversion = \"1.0\"\n",
        );
        write(
            "packages/app/src/acme_app/helpers.py",
            "def helper():\n    return 1\n",
        );
        write(
            "packages/app/src/acme_app/main.py",
            r#"from acme_lib.dates import fmt_date
from .helpers import helper

def run():
    helper()
    return fmt_date(1)
"#,
        );

        let ctx = extract_context(dir.path()).expect("extraction");
        let has_file_edge = |src: &str, dst: &str| {
            ctx.file_edges.iter().any(|e| e.src == src && e.dst == dst)
        };
        assert!(
            has_file_edge(
                "packages/app/src/acme_app/main.py",
                "packages/lib/src/acme_lib/dates.py"
            ),
            "cross-package import edge missing; edges: {:?}",
            ctx.file_edges
        );
        assert!(
            has_file_edge(
                "packages/app/src/acme_app/main.py",
                "packages/app/src/acme_app/helpers.py"
            ),
            "relative import edge missing; edges: {:?}",
            ctx.file_edges
        );
        let key_of = |name: &str| {
            ctx.files
                .iter()
                .flat_map(|f| &f.symbols)
                .find(|s| s.name == name)
                .map(|s| s.key.clone())
                .unwrap()
        };
        let run = key_of("run");
        for dst in ["fmt_date", "helper"] {
            let dst = key_of(dst);
            assert!(
                ctx.symbol_edges.iter().any(|e| e.src == run && e.dst == dst),
                "symbol edge run -> {dst} missing; edges: {:?}",
                ctx.symbol_edges
            );
        }
    }

    /// End-to-end on a scratch Go module: qualified references resolve
    /// through go.mod + import bindings from a plain `extract_context` walk,
    /// and structs/interfaces come out as symbols (the old generic fallback
    /// produced neither).
    #[test]
    fn extracts_go_module_import_edges() {
        let dir = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("go.mod", "module github.com/acme/proj\n\ngo 1.22\n");
        write(
            "internal/db/store.go",
            r#"package db

type Store struct {
	Name string
}

func Connect(dsn string) *Store { return &Store{Name: dsn} }
"#,
        );
        write(
            "cmd/api/main.go",
            r#"package main

import (
	"fmt"

	database "github.com/acme/proj/internal/db"
)

func main() {
	s := database.Connect("dsn")
	var t database.Store
	fmt.Println(s, t)
}
"#,
        );

        let ctx = extract_context(dir.path()).expect("extraction");
        assert!(
            ctx.file_edges
                .iter()
                .any(|e| e.src == "cmd/api/main.go" && e.dst == "internal/db/store.go"),
            "qualified-ref file edge missing; edges: {:?}",
            ctx.file_edges
        );
        let key_of = |name: &str| {
            ctx.files
                .iter()
                .flat_map(|f| &f.symbols)
                .find(|s| s.name == name)
                .map(|s| s.key.clone())
                .unwrap_or_else(|| panic!("symbol {name} missing"))
        };
        // Struct symbols exist (audit: type_spec never matched before)…
        let store = key_of("Store");
        let main = key_of("main");
        // …and both the call and the type reference yield symbol edges.
        for dst in [key_of("Connect"), store] {
            assert!(
                ctx.symbol_edges
                    .iter()
                    .any(|e| e.src == main && e.dst == dst),
                "symbol edge main -> {dst} missing; edges: {:?}",
                ctx.symbol_edges
            );
        }
    }

    /// The claimed-territory listing enumerates EVERY project file — parseable
    /// or not — skipping only dependency/build directories.
    #[test]
    fn project_file_listing_covers_unparseable_files_too() {
        let dir = tempfile::tempdir().unwrap();
        let write = |rel: &str| {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "x").unwrap();
        };
        write("src/main.ts");
        write("README.md");
        write("assets/logo.svg");
        write("node_modules/pkg/index.js");
        write("target/debug/out.rs");

        let files = list_project_files(dir.path());
        assert!(files.contains("src/main.ts"));
        assert!(files.contains("README.md"), "non-source files are listed");
        assert!(files.contains("assets/logo.svg"));
        assert!(!files.contains("node_modules/pkg/index.js"));
        assert!(!files.contains("target/debug/out.rs"));
    }

    #[test]
    fn repeated_extraction_reuses_unchanged_parses() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"cached-project"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("main.ts"),
            "export function run() { return 1; }",
        )
        .unwrap();

        let (_, first) = extract_context_with_stats(dir.path()).expect("first extraction");
        let (_, second) = extract_context_with_stats(dir.path()).expect("second extraction");
        assert_eq!(first.parsed_files, 1);
        assert_eq!(second.parsed_files, 0);
        assert_eq!(second.cache_hits, 1);

        std::fs::write(
            dir.path().join("main.ts"),
            "export function run() { return 2; }",
        )
        .unwrap();
        let (_, changed) = extract_context_with_stats(dir.path()).expect("changed extraction");
        assert_eq!(changed.parsed_files, 1);
        assert_eq!(changed.cache_hits, 0);
    }
}

