use std::path::PathBuf;

use crate::{highlight, symbols};

#[tauri::command]
pub(crate) fn open_in_editor(
    file: String,
    line: Option<u32>,
    symbol: Option<String>,
    project_path: Option<String>,
) -> Result<(), String> {
    // Resolve absolute path
    let path = {
        let p = PathBuf::from(&file);
        if p.is_absolute() {
            p
        } else if let Some(base) = project_path {
            PathBuf::from(base).join(p)
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(p)
        }
    };

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    // A symbol-only anchor (code identifier or a test's name string) still
    // deserves a landing line: resolve it the way the peek does.
    let line = line.or_else(|| {
        let sym = symbol.as_deref().filter(|s| !s.is_empty())?;
        let source = std::fs::read_to_string(&path).ok()?;
        crate::symbols::resolve(&path, &source, sym, None).map(|(start, _)| start)
    });

    let path_str = path.to_string_lossy();

    // Resolve editor: $VISUAL → $EDITOR → auto-detect → fallback
    // Skip TUI editors — we're a GUI app, can't spawn them
    let is_tui = |name: &str| {
        let base = PathBuf::from(name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        matches!(base.as_str(), "vim" | "nvim" | "vi" | "nano" | "emacs" | "helix" | "hx" | "ed" | "micro")
    };
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|v| !is_tui(v))
        .or_else(|| std::env::var("EDITOR").ok().filter(|v| !is_tui(v)))
        .or_else(|| {
            ["code", "cursor", "zed", "zeditor", "subl"]
                .iter()
                .find(|name| which::which(name).is_ok())
                .map(|s| s.to_string())
        });

    let editor = match editor {
        Some(e) => e,
        None => {
            // Fallback: open on macOS, start on Windows, xdg-open on Linux
            if cfg!(target_os = "windows") {
                std::process::Command::new("cmd")
                    .args(["/C", "start", "", &*path_str])
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| format!("Failed to open file: {e}"))?;
            } else {
                let fallback = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                std::process::Command::new(fallback)
                    .arg(&*path_str)
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .map_err(|e| format!("Failed to open file: {e}"))?;
            }
            return Ok(());
        }
    };

    // Extract the binary name for line-number format lookup
    let editor_name = PathBuf::from(&editor)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| editor.clone());

    let mut args: Vec<String> = Vec::new();

    match editor_name.as_str() {
        "code" | "cursor" => {
            args.push("--reuse-window".to_string());
            if let Some(l) = line {
                args.push("--goto".to_string());
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        "zed" | "zeditor" => {
            // -a adds the file to the currently focused workspace
            args.push("-a".to_string());
            if let Some(l) = line {
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        "subl" => {
            if let Some(l) = line {
                args.push(format!("{path_str}:{l}"));
            } else {
                args.push(path_str.to_string());
            }
        }
        _ => {
            args.push(path_str.to_string());
        }
    }

    std::process::Command::new(&editor)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch {editor}: {e}"))?;

    Ok(())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceSpan {
    /// Path actually read, relative form echoed back for display.
    file: String,
    /// 1-based line number of the first returned line (includes context).
    start_line: u32,
    /// The mapped span (for highlighting), 1-based inclusive.
    focus_start: u32,
    focus_end: u32,
    /// Lines from `start_line` onward (context + focus), each a list of
    /// syntax-highlighted segments that concatenate back to the line.
    lines: Vec<Vec<highlight::Segment>>,
}

/// Language-agnostic fallback when no bundled grammar covers the file (or the
/// grammar misses the symbol): the first line that defines `symbol` (word-
/// boundary match next to a definition cue or an assignment/open-paren).
/// Returns the 1-based line.
fn text_search_symbol(lines: &[&str], symbol: &str) -> Option<u32> {
    let cues = [
        "fn ", "function", "def ", "class ", "struct ", "interface ", "enum ",
        "impl", "type ", "const ", "let ", "var ", "func ", "public", "private",
        "export", "module", "trait ", "object ", "sub ", "proc ",
    ];
    for (i, raw) in lines.iter().enumerate() {
        // word-boundary occurrence of the symbol
        let Some(pos) = raw.find(symbol) else { continue };
        let before_ok = pos == 0
            || !raw.as_bytes()[pos - 1].is_ascii_alphanumeric() && raw.as_bytes()[pos - 1] != b'_';
        let after_idx = pos + symbol.len();
        let after_ok = after_idx >= raw.len()
            || (!raw.as_bytes()[after_idx].is_ascii_alphanumeric() && raw.as_bytes()[after_idx] != b'_');
        if !(before_ok && after_ok) {
            continue;
        }
        let trimmed = raw.trim_start();
        let after = raw[after_idx..].trim_start();
        let looks_def = cues.iter().any(|c| trimmed.starts_with(c) || raw.contains(c))
            || after.starts_with('(')
            || after.starts_with('=')
            || after.starts_with(':')
            || after.starts_with('<');
        if looks_def {
            return Some(i as u32 + 1);
        }
    }
    None
}

/// Read a span of a source file for the inspector's code view.
///
/// `file` is the `SourceLocation.pattern`. The responsibility's *focus* is the
/// explicit `line`/`end_line` range — the statements that do its work. `symbol`
/// names the enclosing definition: it's the durable anchor (so the focus can be
/// shown even as line numbers drift) and bounds what we render — the whole
/// symbol body is returned with the focus lines flagged, so you read the focus
/// in full context. When only `symbol` is given, the whole definition is the
/// focus. With no symbol, the whole file is returned. The fixed-height scroll
/// viewport bounds the visual size, so the data is never truncated. Reads are
/// constrained to within `project_path`.
#[tauri::command]
pub(crate) fn read_source_span(
    project_path: String,
    file: String,
    symbol: Option<String>,
    line: Option<u32>,
    end_line: Option<u32>,
) -> Result<SourceSpan, String> {
    const NO_LINE_LIMIT: u32 = 40;
    const DEFAULT_SPAN: u32 = 30;

    let base = PathBuf::from(&project_path);
    let path = base.join(&file);

    // Constrain to the project directory (reject path traversal / absolutes).
    let canon_base = base.canonicalize().map_err(|e| e.to_string())?;
    let canon = path
        .canonicalize()
        .map_err(|e| format!("{}: {}", file, e))?;
    if !canon.starts_with(&canon_base) {
        return Err(format!("{} is outside the project", file));
    }

    let contents = std::fs::read_to_string(&canon).map_err(|e| format!("{}: {}", file, e))?;
    let all: Vec<&str> = contents.lines().collect();
    let total = all.len() as u32;
    if total == 0 {
        return Ok(SourceSpan {
            file,
            start_line: 1,
            focus_start: 1,
            focus_end: 1,
            lines: Vec::new(),
        });
    }

    // Enclosing symbol span: tree-sitter first (exact body), then a
    // language-agnostic text search (start line + a default window).
    let sym_range: Option<(u32, u32)> =
        symbol.as_deref().filter(|s| !s.is_empty()).and_then(|s| {
            symbols::resolve(&canon, &contents, s, line)
                .or_else(|| text_search_symbol(&all, s).map(|st| (st, (st + DEFAULT_SPAN).min(total))))
        });

    // Focus: the responsibility's specific lines if given, else the whole
    // enclosing symbol, else the file head.
    let (focus_start, focus_end) = match line {
        Some(l) => {
            let fs = l.clamp(1, total);
            (fs, end_line.unwrap_or(fs).clamp(fs, total))
        }
        None => match sym_range {
            Some((s, e)) => (s, e),
            None => (1, NO_LINE_LIMIT.min(total)),
        },
    };

    // Render window: the whole enclosing symbol body (so the focus is always
    // read in full context), or the whole file when no symbol resolves. The
    // focus is always contained. The fixed-height scroll viewport on the
    // frontend bounds visual size, so we never truncate — that only ever hid
    // lines and lied about the range.
    let (start, end) = match sym_range {
        Some((ss, se)) => (ss.min(focus_start), se.max(focus_end).min(total)),
        None => (1, total),
    };

    // Syntax-highlight the whole file (line N → index N-1), falling back to
    // plain default-coloured segments for languages without a grammar, then
    // slice out the render window.
    let highlighted = highlight::highlight_lines(&canon, &contents).unwrap_or_else(|| {
        all.iter()
            .map(|l| {
                vec![highlight::Segment {
                    text: l.to_string(),
                    kind: String::new(),
                }]
            })
            .collect()
    });
    let lines: Vec<Vec<highlight::Segment>> = highlighted
        .into_iter()
        .skip(start as usize - 1)
        .take((end - start + 1) as usize)
        .collect();

    Ok(SourceSpan {
        file,
        start_line: start,
        focus_start,
        focus_end,
        lines,
    })
}

/// Whether a single source-map anchor currently lands in real code. The model's
/// source map records intent (a file plus an optional symbol/line); this reports
/// whether that intent resolves right now, so the inspector can distinguish an
/// anchor backed by code from one pointing at a symbol or file that isn't there
/// (yet) — e.g. a spec authored before the code exists.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AnchorStatus {
    /// The symbol resolves (or the line is in range) in an existing file.
    Resolved,
    /// The file doesn't exist on disk.
    FileMissing,
    /// The file exists but the named symbol isn't found in it.
    SymbolMissing,
    /// A line-only anchor points past the end of the file.
    LineOutOfRange,
}

/// Verify whether one source anchor resolves to real code. Mirrors how
/// `read_source_span` locates a symbol (tree-sitter, then a text-search
/// fallback) but renders nothing — it's the cheap pre-flight the inspector uses
/// to pick the anchor icon. Only anchored locations (a symbol or a line) are
/// meaningful; a whole-file mapping shouldn't call this. A missing or escaping
/// file reads as `FileMissing` rather than erroring, since "not there yet" is a
/// normal state for an authored-ahead spec.
#[tauri::command]
pub(crate) fn verify_anchor(
    project_path: String,
    file: String,
    symbol: Option<String>,
    line: Option<u32>,
) -> AnchorStatus {
    let base = PathBuf::from(&project_path);
    let path = base.join(&file);

    let Ok(canon_base) = base.canonicalize() else {
        return AnchorStatus::FileMissing;
    };
    let Ok(canon) = path.canonicalize() else {
        return AnchorStatus::FileMissing;
    };
    if !canon.starts_with(&canon_base) {
        return AnchorStatus::FileMissing;
    }
    let Ok(contents) = std::fs::read_to_string(&canon) else {
        return AnchorStatus::FileMissing;
    };

    // A named symbol is the durable anchor: resolve it the same way the peek
    // does (tree-sitter, then text search). Otherwise fall back to the line.
    if let Some(sym) = symbol.as_deref().filter(|s| !s.is_empty()) {
        let lines: Vec<&str> = contents.lines().collect();
        let found = symbols::resolve(&canon, &contents, sym, line).is_some()
            || text_search_symbol(&lines, sym).is_some();
        return if found {
            AnchorStatus::Resolved
        } else {
            AnchorStatus::SymbolMissing
        };
    }

    match line {
        Some(l) => {
            let total = contents.lines().count() as u32;
            if (1..=total).contains(&l) {
                AnchorStatus::Resolved
            } else {
                AnchorStatus::LineOutOfRange
            }
        }
        // No symbol and no line isn't an anchor; the file exists, so it's fine.
        None => AnchorStatus::Resolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RS: &str = "fn other() {}\n\nfn target() {\n    let a = 1;\n    let b = 2;\n}\n";

    fn project() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/m.rs"), RS).unwrap();
        let p = dir.path().to_string_lossy().to_string();
        (dir, p)
    }

    /// The inspector's span: the whole enclosing symbol body is returned with
    /// the claim's focus lines flagged inside it.
    #[test]
    fn span_returns_the_enclosing_symbol_with_the_focus_flagged() {
        let (_dir, project) = project();
        let span = read_source_span(
            project,
            "src/m.rs".into(),
            Some("target".into()),
            Some(4),
            Some(5),
        )
        .unwrap();
        assert_eq!(span.start_line, 3, "window opens at the symbol");
        assert_eq!((span.focus_start, span.focus_end), (4, 5), "focus lines flagged");
        let first_line: String = span.lines[0].iter().map(|s| s.text.as_str()).collect();
        assert!(first_line.contains("fn target"), "{first_line}");
        assert!(span.lines.len() >= 4, "whole symbol body returned");
    }

    /// A read that would escape the project directory is rejected.
    #[test]
    fn a_read_escaping_the_project_is_rejected() {
        let outer = tempfile::tempdir().unwrap();
        let project_dir = outer.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(outer.path().join("secret.txt"), "s3cret").unwrap();

        let err = read_source_span(
            project_dir.to_string_lossy().to_string(),
            "../secret.txt".into(),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("outside the project"), "{err}");
    }

    /// The anchor pre-flight distinguishes every state: resolved, symbol gone,
    /// file gone, line past the end.
    #[test]
    fn anchor_verification_reports_each_state() {
        let (_dir, project) = project();
        let verify = |file: &str, symbol: Option<&str>, line: Option<u32>| {
            serde_json::to_value(verify_anchor(
                project.clone(),
                file.into(),
                symbol.map(String::from),
                line,
            ))
            .unwrap()
        };
        assert_eq!(verify("src/m.rs", Some("target"), None), "resolved");
        assert_eq!(verify("src/m.rs", Some("vanished"), None), "symbolMissing");
        assert_eq!(verify("src/gone.rs", Some("target"), None), "fileMissing");
        assert_eq!(verify("src/m.rs", None, Some(3)), "resolved");
        assert_eq!(verify("src/m.rs", None, Some(99)), "lineOutOfRange");
    }

    /// With no grammar for the file, a text search finds the first line that
    /// DEFINES the symbol — a mere mention doesn't count.
    #[test]
    fn text_search_finds_the_defining_line_not_a_mention() {
        let lines = vec!["-- talks about total sums", "local total = 5", "print(total)"];
        assert_eq!(text_search_symbol(&lines, "total"), Some(2), "the assignment defines it");
        let lines = vec!["// about widgets", "widgets are nice", "def widget():", "    pass"];
        assert_eq!(text_search_symbol(&lines, "widget"), Some(3));
        assert_eq!(text_search_symbol(&lines, "gadget"), None);
    }

    /// Jumping to source resolves the anchor's line and hands the file to the
    /// configured GUI editor — a TUI $VISUAL is skipped for the GUI $EDITOR.
    #[test]
    fn jump_resolves_the_line_and_skips_tui_editors() {
        use std::os::unix::fs::PermissionsExt as _;
        let (_dir, project) = project();
        let bin = tempfile::tempdir().unwrap();
        // A fake Sublime: `subl` gets the `path:line` argument form.
        let args_file = bin.path().join("args.txt");
        let editor = bin.path().join("subl");
        std::fs::write(
            &editor,
            format!("#!/bin/sh\necho \"$@\" > '{}'\n", args_file.display()),
        )
        .unwrap();
        std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::env::set_var("VISUAL", "vim"); // TUI — must be skipped
        std::env::set_var("EDITOR", editor.to_string_lossy().to_string());
        let result = open_in_editor(
            "src/m.rs".into(),
            None,
            Some("target".into()),
            Some(project.clone()),
        );
        std::env::remove_var("VISUAL");
        std::env::remove_var("EDITOR");
        result.unwrap();

        let mut args = String::new();
        for _ in 0..100 {
            if let Ok(a) = std::fs::read_to_string(&args_file) {
                args = a;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            args.trim().ends_with("src/m.rs:3"),
            "the symbol's line rides the jump: {args:?}"
        );

        let err = open_in_editor("src/gone.rs".into(), None, None, Some(project)).unwrap_err();
        assert!(err.contains("File not found"), "{err}");
    }
}
