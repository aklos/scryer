//! The desktop app's wiring: recover the user's PATH (a macOS app launched
//! from the Dock or Finder inherits a minimal one), build the Tauri app,
//! register its managed state and its whole command surface.

use std::sync::Mutex;

use tauri::Manager;

use crate::shell::state::{AcpState, PreviewState, WatcherState};
use crate::shell::{derived, mcp_setup, observability, preview, project, source_view};

/// macOS GUI apps launched via Spotlight, Dock, or Finder inherit a minimal
/// PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) that excludes user-installed tools.
/// Recover the user's real PATH by asking their login shell directly.
#[cfg(target_os = "macos")]
fn ensure_full_path() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());

        // Fish outputs PATH as space-separated; ask it for colon-separated
        let echo_cmd = if shell.ends_with("/fish") {
            "string join : $PATH"
        } else {
            "echo $PATH"
        };

        let Ok(output) = std::process::Command::new(&shell)
            .args(["-l", "-c", echo_cmd])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return;
        };

        if output.status.success() {
            let shell_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !shell_path.is_empty() {
                std::env::set_var("PATH", &shell_path);
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AcpState(
            Mutex::new(None),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        ))
        .setup(move |app| {
            app.manage(Mutex::new(WatcherState { project: None }));
            app.manage(PreviewState(tokio::sync::Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project::watch_project,
            project::read_model,
            project::read_planned,
            source_view::open_in_editor,
            source_view::read_source_span,
            source_view::verify_anchor,
            mcp_setup::detect_ai_tools,
            mcp_setup::agent_integration,
            mcp_setup::set_agent_integration,
            project::get_subagent_settings,
            project::set_subagent_settings,
            preview::ensure_preview_server,
            preview::start_preview_fixture_session,
            observability::get_model_health,
            observability::get_test_statuses,
            observability::get_probe_statuses,
            preview::cancel_agent_session,
            derived::get_derived_status,
            derived::get_landing,
            derived::get_worktrees,
            derived::list_worktrees,
            derived::edit_claim_text,
            derived::add_claim_stub,
            derived::rename_container,
            derived::list_comments,
            derived::open_comment,
            derived::reply_comment,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_, _| {});
}
