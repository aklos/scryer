//! tsconfig/jsconfig path-alias discovery.
//!
//! TypeScript projects remap bare import specifiers through
//! `compilerOptions.paths` (patterns with at most one `*`) resolved against
//! `baseUrl` or the declaring config's directory, and configs inherit through
//! `extends`. This module finds every governing config in the repo, follows
//! relative `extends` chains (an npm-package base is unresolvable without
//! `node_modules` and is skipped — guessing would mint false aliases), and
//! flattens each into a project-relative alias table the edge resolver can
//! apply without touching the filesystem.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

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

/// Discover every alias-bearing tsconfig/jsconfig among `all_files`. Configs
/// in one directory merge (plain `tsconfig.json` first, then variants like
/// Vite's `tsconfig.app.json` or Nx's `tsconfig.base.json`, first declaration
/// of a pattern wins); directories whose configs declare no aliases yield
/// nothing.
pub fn discover_ts_aliases(project: &Path, all_files: &[PathBuf]) -> Vec<TsAliases> {
    let mut by_dir: BTreeMap<String, Vec<&PathBuf>> = BTreeMap::new();
    for path in all_files {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let is_config = name == "tsconfig.json"
            || name == "jsconfig.json"
            || (name.starts_with("tsconfig.") && name.ends_with(".json"));
        if !is_config {
            continue;
        }
        let Ok(rel) = path.strip_prefix(project) else {
            continue;
        };
        let dir = rel
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        by_dir.entry(dir).or_default().push(path);
    }

    let mut out = Vec::new();
    for (dir, mut configs) in by_dir {
        // Plain tsconfig.json first (it usually `extends` the variants and is
        // what tsc actually loads), then the rest in name order — deterministic.
        configs.sort_by_key(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            (name != "tsconfig.json", name.to_string())
        });
        let mut merged = TsAliases {
            dir,
            ..Default::default()
        };
        for config in configs {
            let Some(resolved) = resolve_config(project, config) else {
                continue;
            };
            if merged.base_url.is_none() {
                merged.base_url = resolved.base_url;
            }
            for (pattern, targets) in resolved.paths {
                if !merged.paths.iter().any(|(p, _)| *p == pattern) {
                    merged.paths.push((pattern, targets));
                }
            }
        }
        if merged.base_url.is_some() || !merged.paths.is_empty() {
            out.push(merged);
        }
    }
    out
}

/// Flatten one config file plus its `extends` chain into a project-relative
/// alias table. TS field semantics: the child's `baseUrl`/`paths` override a
/// parent's WHOLESALE (no per-key merge); `paths` targets resolve against the
/// effective `baseUrl` when one is declared, else against the directory of the
/// config that declared them.
fn resolve_config(project: &Path, config_path: &Path) -> Option<TsAliases> {
    let mut chain: Vec<(PathBuf, serde_json::Value)> = Vec::new();
    load_chain(config_path, &mut HashSet::new(), &mut chain);

    // First declaration along the chain wins (chain is child-first).
    let mut base_url: Option<(PathBuf, String)> = None;
    let mut paths_decl: Option<(PathBuf, Vec<(String, Vec<String>)>)> = None;
    for (config_dir, value) in &chain {
        let Some(options) = value.get("compilerOptions") else {
            continue;
        };
        if base_url.is_none() {
            if let Some(b) = options.get("baseUrl").and_then(|v| v.as_str()) {
                base_url = Some((config_dir.clone(), b.to_string()));
            }
        }
        if paths_decl.is_none() {
            if let Some(map) = options.get("paths").and_then(|v| v.as_object()) {
                let list = map
                    .iter()
                    .map(|(pattern, targets)| {
                        let targets = targets
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|t| t.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (pattern.clone(), targets)
                    })
                    .collect();
                paths_decl = Some((config_dir.clone(), list));
            }
        }
    }

    // A declaring directory outside the project cannot yield project-relative
    // aliases — drop that declaration rather than guess.
    let rel_dir = |abs: &Path| -> Option<String> {
        abs.strip_prefix(project)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
    };
    let base_url_rel = base_url
        .as_ref()
        .and_then(|(dir, b)| crate::manifest::resolve_rel(&rel_dir(dir)?, b));

    let paths = paths_decl
        .and_then(|(dir, list)| {
            let target_base = base_url_rel.clone().or_else(|| rel_dir(&dir))?;
            let resolved = list
                .into_iter()
                .filter_map(|(pattern, targets)| {
                    let targets: Vec<String> = targets
                        .iter()
                        .filter_map(|t| crate::manifest::resolve_rel(&target_base, t))
                        .collect();
                    (!targets.is_empty()).then_some((pattern, targets))
                })
                .collect::<Vec<_>>();
            Some(resolved)
        })
        .unwrap_or_default();

    Some(TsAliases {
        dir: rel_dir(config_path.parent()?)?,
        base_url: base_url_rel,
        paths,
    })
}

/// Read a config and its `extends` ancestry, child-first. Relative bases only;
/// an array of bases (TS 5) queues in reverse so that later entries — which TS
/// gives higher precedence — are scanned earlier by the first-wins pass.
fn load_chain(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    out: &mut Vec<(PathBuf, serde_json::Value)>,
) {
    if visited.len() > 16 {
        return; // depth backstop far above any real chain
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(canonical) {
        return; // extends cycle
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(&text)) else {
        return;
    };
    let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();

    let bases: Vec<String> = match value.get("extends") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .rev()
            .collect(),
        _ => Vec::new(),
    };
    out.push((dir.clone(), value));

    for base in bases {
        if !(base.starts_with("./") || base.starts_with("../")) {
            continue; // an npm-package base needs node_modules — skip, don't guess
        }
        // Collapse `..`/`.` lexically: the DECLARING DIRECTORY of inherited
        // fields is this path's parent, and it must strip against the project
        // root cleanly ("packages/app/../.." never would).
        let mut target = normalize_lexically(&dir.join(&base));
        if target.extension().is_none() {
            target.set_extension("json");
        }
        load_chain(&target, visited, out);
    }
}

/// Remove `.` and collapse `..` path components without touching the
/// filesystem (symlink-exact resolution isn't needed for config identity).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut parts: Vec<std::path::Component> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
                if matches!(parts.last(), Some(std::path::Component::Normal(_))) =>
            {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

/// Strip `//` and `/* */` comments plus trailing commas from JSONC,
/// string-aware in both passes (a `//` inside a string literal survives).
fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            out.push(b);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_string = true;
                out.push(b);
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }

    let mut cleaned: Vec<u8> = Vec::with_capacity(out.len());
    let mut in_string = false;
    let mut i = 0;
    while i < out.len() {
        let b = out[i];
        if in_string {
            cleaned.push(b);
            if b == b'\\' && i + 1 < out.len() {
                cleaned.push(out[i + 1]);
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
        } else if b == b',' {
            let next = out[i + 1..]
                .iter()
                .find(|c| !c.is_ascii_whitespace())
                .copied();
            if matches!(next, Some(b'}') | Some(b']')) {
                i += 1;
                continue; // trailing comma
            }
        }
        cleaned.push(b);
        i += 1;
    }
    String::from_utf8(cleaned).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn strips_comments_and_trailing_commas_string_aware() {
        let jsonc = r#"{
  // line comment
  "url": "http://x/*not-a-comment*/y", /* block */
  "list": [1, 2, ],
}"#;
        let value: serde_json::Value = serde_json::from_str(&strip_jsonc(jsonc)).unwrap();
        assert_eq!(
            value["url"].as_str().unwrap(),
            "http://x/*not-a-comment*/y"
        );
        assert_eq!(value["list"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn discovers_paths_and_baseurl() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = write(
            tmp.path(),
            "tsconfig.json",
            r#"{
  "compilerOptions": {
    "baseUrl": ".", // comment
    "paths": { "@/*": ["./src/*"], "config": ["./src/config.ts"], },
  },
}"#,
        );
        let aliases = discover_ts_aliases(tmp.path(), &[cfg]);
        assert_eq!(aliases.len(), 1);
        let a = &aliases[0];
        assert_eq!(a.dir, "");
        assert_eq!(a.base_url.as_deref(), Some(""));
        assert!(a
            .paths
            .iter()
            .any(|(p, t)| p == "@/*" && t == &["src/*".to_string()]));
        assert!(a
            .paths
            .iter()
            .any(|(p, t)| p == "config" && t == &["src/config.ts".to_string()]));
    }

    /// Nx shape: aliases live in a root `tsconfig.base.json`; the package
    /// config `extends` it. The child's declared fields override wholesale;
    /// the inherited `paths` resolve against the DECLARING config's dir.
    #[test]
    fn extends_chain_inherits_and_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "tsconfig.base.json",
            r#"{ "compilerOptions": { "paths": { "@acme/ui": ["./packages/ui/src/index.ts"] } } }"#,
        );
        let child = write(
            tmp.path(),
            "packages/app/tsconfig.json",
            r#"{ "extends": "../../tsconfig.base.json",
                 "compilerOptions": { "baseUrl": "./src" } }"#,
        );
        let aliases = discover_ts_aliases(tmp.path(), &[child]);
        assert_eq!(aliases.len(), 1);
        let a = &aliases[0];
        assert_eq!(a.dir, "packages/app");
        // Child's own baseUrl…
        assert_eq!(a.base_url.as_deref(), Some("packages/app/src"));
        // …and here baseUrl also rebases the inherited targets (TS resolves
        // paths against the effective baseUrl when one is declared).
        assert_eq!(
            a.paths,
            vec![(
                "@acme/ui".to_string(),
                vec!["packages/app/src/packages/ui/src/index.ts".to_string()]
            )]
        );
    }

    /// Vite shape: a solution-style `tsconfig.json` with no options next to a
    /// `tsconfig.app.json` holding the aliases — both merge into one table.
    /// An npm-package `extends` is skipped, not guessed.
    #[test]
    fn sibling_variant_configs_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let solution = write(
            tmp.path(),
            "tsconfig.json",
            r#"{ "extends": "@tsconfig/node18", "files": [] }"#,
        );
        let app = write(
            tmp.path(),
            "tsconfig.app.json",
            r#"{ "compilerOptions": { "paths": { "@/*": ["./src/*"] } } }"#,
        );
        let aliases = discover_ts_aliases(tmp.path(), &[solution, app]);
        assert_eq!(aliases.len(), 1);
        assert_eq!(
            aliases[0].paths,
            vec![("@/*".to_string(), vec!["src/*".to_string()])]
        );
    }

    #[test]
    fn extends_cycle_terminates() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "a.json",
            r#"{ "extends": "./tsconfig.json" }"#,
        );
        let cfg = write(
            tmp.path(),
            "tsconfig.json",
            r#"{ "extends": "./a.json", "compilerOptions": { "baseUrl": "." } }"#,
        );
        let aliases = discover_ts_aliases(tmp.path(), &[cfg]);
        assert_eq!(aliases.len(), 1); // terminated, config still read
    }
}
