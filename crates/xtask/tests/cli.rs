//! The dev-tasks CLI's dispatch: an unknown (or missing) task prints usage
//! and fails.

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
