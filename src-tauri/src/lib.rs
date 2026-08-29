mod build;
mod highlight;
mod hooks;
mod mcp_setup;
mod observability;
mod preview;
mod project;
mod source_view;
mod state;
mod symbols;
mod test_reports;
mod verdicts;

use std::sync::Mutex;

use tauri::Manager;

use state::{AcpState, PreviewState, WatcherState};

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
    #[cfg(target_os = "macos")]
    ensure_full_path();

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
            app.manage(hooks::HookState(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            project::watch_project,
            project::is_legacy_model,
            project::read_model,
            project::read_planned,
            project::write_planned,
            project::close_change,
            project::read_history,
            source_view::open_in_editor,
            source_view::read_source_span,
            source_view::verify_anchor,
            mcp_setup::detect_ai_tools,
            mcp_setup::setup_mcp_integration,
            project::create_blank_model,
            project::get_subagent_settings,
            project::set_subagent_settings,
            preview::ensure_preview_server,
            preview::start_preview_fixture_session,
            build::start_model_build,
            build::start_drift_check,
            observability::get_drift_status,
            observability::get_model_health,
            observability::get_test_statuses,
            observability::get_probe_statuses,
            observability::reconcile_drift,
            observability::reconcile_drift_node,
            verdicts::adopt_responsibility,
            verdicts::reject_responsibility,
            verdicts::drop_responsibility,
            verdicts::reimplement_responsibility,
            verdicts::adopt_property,
            verdicts::reject_property,
            verdicts::drop_property,
            verdicts::reimplement_property,
            verdicts::reword_responsibility,
            verdicts::drop_node,
            verdicts::reimplement_node,
            preview::cancel_agent_session,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Drop the hook endpoint on exit so its discovery file is removed
            // and session hooks fall silent the moment the app closes.
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<hooks::HookState>() {
                    *state.0.lock().unwrap() = None;
                }
            }
        });
}
