//! The desktop app's wiring: recover the user's PATH (a macOS app launched
//! from the Dock or Finder inherits a minimal one), build the Tauri app,
//! register its managed state and its whole command surface, and take
//! `scryer://` links: one app runs at a time, and a link clicked while it runs
//! goes to it rather than to a second copy.

use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

use crate::shell::state::{AcpState, PreviewState, WatcherState};
use crate::shell::{authored_model, derived, links, mcp_setup, observability, preview, project, source_view};

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
        // First, so a second launch (a link clicked while the app runs) hands its arguments over and exits; with the
        // deep-link feature the link reaches `on_open_url` below.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.webview_windows().values().next() {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AcpState(
            Mutex::new(None),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        ))
        .setup(move |app| {
            app.manage(Mutex::new(WatcherState { project: None }));
            app.manage(PreviewState(tokio::sync::Mutex::new(None)));
            app.manage(links::PendingLink::default());
            // Installed packages register the scheme; a dev build or a bare AppImage registers itself, so the copy
            // that ran last takes the links.
            #[cfg(any(target_os = "linux", windows))]
            let _ = app.deep_link().register_all();
            #[cfg(target_os = "linux")]
            links::ensure_default_handler();
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
                links::receive(&handle, &urls);
            });
            // Launched by a link: it is in the arguments, before anything listens.
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                let urls: Vec<String> = urls.iter().map(|u| u.to_string()).collect();
                links::receive(app.handle(), &urls);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project::watch_project,
            project::get_session_brief,
            links::take_link,
            project::read_change_base,
            project::read_base_model,
            project::read_planned,
            source_view::open_in_editor,
            source_view::read_source_span,
            source_view::verify_anchor,
            mcp_setup::detect_ai_tools,
            mcp_setup::agent_integration,
            mcp_setup::set_agent_integration,
            authored_model::authored_model,
            authored_model::migrate_authored_model,
            authored_model::remove_authored_model,
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
            derived::get_history,
            derived::get_commit_diff,
            derived::get_uncommitted_diff,
            derived::list_worktrees,
            derived::edit_claim_text,
            derived::add_claim_stub,
            derived::rename_node,
            derived::edit_node_description,
            derived::edit_node_decisions,
            derived::list_comments,
            derived::open_comment,
            derived::reply_comment,
            derived::delete_comment,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // The agents' MCP servers list `show` while the app has the
            // project open; a clean exit takes the record with it.
            if let tauri::RunEvent::Exit = event {
                project::forget_open_project(&app.state::<Mutex<WatcherState>>());
            }
        });
}
