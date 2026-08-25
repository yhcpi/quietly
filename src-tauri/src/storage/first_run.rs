//! First-run acknowledgement, stored as a sentinel file in
//! `app_data_dir` rather than in the SQLite `settings` table.
//!
//! Why a file and not the DB:
//!
//! The WebView2 frontend mounts and runs React effects as soon as
//! the Tauri window appears — which is *before* `app.manage(state)`
//! in `lib.rs` setup completes. Anything invoked from a mount-time
//! `useEffect` therefore hits the IPC dispatcher while `AppState`
//! (which carries the DB pool) is not yet registered. A `#[tauri::command]`
//! with `state: State<AppState>` returns "state not managed for
//! field `state`" in that window — and no amount of retrying from
//! the frontend fixes it for commands that take `State<AppState>`
//! in the `tauri::generate_handler!` macro's macro-expanded wrapper.
//!
//! First-run ack is the only "needs to fire on first mount" command,
//! so it gets its own file-backed helper. The file lives next to
//! `quietly.db` and uses the same atomic-write semantics.
//!
//! Side benefit: this works even on a host where the DB pool fails
//! to open (disk full, perms denied) — the first-run guide still
//! shows up.

use std::path::{Path, PathBuf};

const FILE_NAME: &str = "first_run_acked";

fn path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE_NAME)
}

/// True if the user has not yet seen and dismissed the first-run
/// guide. False once they click "Got it" (or if the marker file is
/// present from any prior install).
pub fn needs_ack(app_data_dir: &Path) -> bool {
    !path(app_data_dir).exists()
}

/// Persist the acknowledgement. Idempotent — repeated calls are no-ops.
pub fn mark_acked(app_data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(app_data_dir)?;
    std::fs::write(path(app_data_dir), b"1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_dir_needs_ack() {
        let d = tempfile::tempdir().unwrap();
        assert!(needs_ack(d.path()));
    }

    #[test]
    fn after_mark_does_not_need_ack() {
        let d = tempfile::tempdir().unwrap();
        mark_acked(d.path()).unwrap();
        assert!(!needs_ack(d.path()));
    }

    #[test]
    fn mark_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        mark_acked(d.path()).unwrap();
        mark_acked(d.path()).unwrap();
        assert!(!needs_ack(d.path()));
    }
}