//! The dev-tasks CLI's dispatch: each invocation routes to its task, and an
//! unknown (or missing) task prints usage and fails.

use std::process::Command;

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
}

/// No task, or an unknown one, is dispatched nowhere: usage on stderr, exit 1.
#[test]
fn an_unknown_task_prints_usage_and_fails() {
    for args in [&[][..], &["frobnicate"][..]] {
        let out = xtask().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("Usage:"));
    }
}

/// `validate-model` dispatches to the validation task: it reads the project's
/// model and runs structural and source-coverage validation against it —
/// a clean model reports clean and exits 0, a missing one fails.
#[test]
fn validate_model_reads_and_validates_the_projects_model() {
    let dir = tempfile::tempdir().unwrap();
    let r = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());
    scryer_core::write_model_at(&r, &scryer_core::ScryModel::new()).unwrap();

    let out = xtask().args(["validate-model", dir.path().to_str().unwrap()]).output().unwrap();
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("clean"));

    let empty = tempfile::tempdir().unwrap();
    let out = xtask().args(["validate-model", empty.path().to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "no model to read must fail");
    assert!(String::from_utf8_lossy(&out.stderr).contains("Failed to read"));
}
