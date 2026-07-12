use std::path::PathBuf;
use std::sync::Mutex;

/// Managed state for the ACP runtime (agent orchestration).
///
/// The `AtomicBool` is a build-scoped cancel flag set DIRECTLY by
/// `cancel_agent_session` (not only via the runtime). Orchestrators reset it at
/// start and check it at every wave/scope boundary, so a "stop" pressed in a
/// no-session gap — or just before a queued parallel session starts — is still
/// honored. Without it, cancellation is edge-triggered on live sessions and gets
/// silently lost in those gaps.
pub(crate) struct AcpState(
    pub(crate) Mutex<Option<scryer_acp::AcpRuntime>>,
    pub(crate) std::sync::Arc<std::sync::atomic::AtomicBool>,
);

/// Managed state for the file watcher — only the active project is watched.
pub(crate) struct WatcherState {
    pub(crate) project: Option<(PathBuf, notify::RecommendedWatcher)>,
}

/// Managed state for the deterministic preview server — one shared Vite dev
/// server per open project (Track B). The child's stdin is held open by the
/// handle; the sidecar exits when the pipe closes, so it can't outlive us.
pub(crate) struct PreviewState(pub(crate) tokio::sync::Mutex<Option<PreviewServer>>);

pub(crate) struct PreviewServer {
    pub(crate) cwd: String,
    pub(crate) url: String,
    pub(crate) child: tokio::process::Child,
}
