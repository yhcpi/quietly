// Persistent marker for "we have set the system proxy to 127.0.0.1:<port>".
//
// Why: when Quietly sets the Windows system proxy via HKCU and the process
// is killed (taskkill /F, crash, power loss), stop_proxy never runs and the
// proxy stays pointing at our dead port. The user's browser then can't reach
// any site until they manually clear the registry.
//
// Defense in depth:
// 1. On every successful start_proxy we write this marker with the snapshot
//    we *would* restore to on stop.
// 2. On stop_proxy we delete the marker (after restoring).
// 3. On app startup, if the marker exists we restore from it and delete it.
//    This handles the crash path: a stale marker is evidence that the prior
//    session died without calling stop_proxy.
//
// The marker is stored under `app_data_dir()/proxy-marker.json` next to
// the SQLite db.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::engine::proxy_state::{self, ProxySnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyMarker {
    /// Port the previous session set the system proxy to. Sanity check
    /// on restore — we never want to clobber a different port that another
    /// tool happened to set.
    pub port: u16,
    /// PID of the process that wrote the marker. Used at reap time to
    /// refuse to restore if the marker is still owned by a live process
    /// (which would mean a second Quietly instance is running and racing
    /// us — restoring here would clobber its proxy settings on shutdown).
    #[serde(default)]
    pub pid: u32,
    /// The proxy state captured BEFORE we set ours. This is what we restore.
    pub snapshot: ProxySnapshot,
}

fn marker_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("proxy-marker.json")
}

pub fn write(app_data_dir: &Path, marker: &ProxyMarker) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(marker)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(marker_path(app_data_dir), json)
}

/// Returns true if a process with the given PID is currently alive on
/// the local machine. Used to refuse to reap a marker that is still
/// owned by a running Quietly instance.
#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    use windows::Win32::System::Threading::OpenProcess;
    use windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;
    // STILL_ACTIVE (259) is the documented exit code returned by a live
    // process to GetExitCodeProcess. Declared as a literal here because
    // the windows crate ships STILL_ACTIVE as a Win32 constant only in
    // some feature combinations; 259 is the documented value across
    // every Windows SDK since NT 4.0.
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        // SAFETY: OpenProcess with QUERY_LIMITED_INFORMATION is a read-only
        // handle — no side effects on the target process. The HANDLE
        // returned is closed when the wrapper is dropped (CloseHandle via
        // Drop).
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut exit_code: u32 = 0;
                let ok = windows::Win32::System::Threading::GetExitCodeProcess(
                    handle, &mut exit_code,
                ).is_ok();
                ok && exit_code == STILL_ACTIVE
            }
            // ERROR_INVALID_PARAMETER (PID does not exist) and
            // ERROR_NOT_FOUND are both reported as Err from OpenProcess;
            // treat them as "not alive". ERROR_ACCESS_DENIED can fire if
            // the process is alive but we lack permission; treat as "alive"
            // so we err on the side of not clobbering it.
            Err(e) => e.code() == windows::Win32::Foundation::E_ACCESSDENIED,
        }
    }
}

#[cfg(not(windows))]
fn pid_is_alive(_pid: u32) -> bool {
    // PID liveness only implemented on Windows in this crate; on other
    // platforms (none are currently supported) skip the check.
    false
}

/// Restore the proxy state recorded in the marker (if any), then delete the
/// marker. Intended to be called once on app startup. Returns:
/// - Ok(Some(marker)) if a stale marker was found and restored
/// - Ok(None) if no marker existed, or the marker is still owned by a live
///   process (left in place for the live owner to clean up)
pub fn reap(app_data_dir: &Path) -> std::io::Result<Option<ProxyMarker>> {
    let path = marker_path(app_data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let mut marker: ProxyMarker = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            // Unparseable marker — treat as gone so we don't loop forever.
            // Log to stderr for visibility; main path doesn't surface this.
            let _ = std::fs::remove_file(&path);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("proxy-marker.json unparseable: {}", e),
            ));
        }
    };
    // Marker files written before the PID field was added carry `pid=0`
    // due to #[serde(default)]. Treat that as "unknown → safe to reap".
    if marker.pid != 0 && pid_is_alive(marker.pid) && marker.pid != std::process::id() {
        // The previous session is still alive — leave the marker alone so
        // it can clean up on its own shutdown. We didn't restore anything.
        return Ok(None);
    }
    // Only restore if the recorded proxy is still pointing at our port.
    // Someone else might have changed it in between; respect that.
    if let Ok(current) = proxy_state::read_current() {
        let expected = format!("127.0.0.1:{}", marker.port);
        if current.server.as_deref() == Some(expected.as_str()) && current.enable {
            // Best-effort restore; ignore proxy failures — we've at least
            // tried, and we're going to delete the marker regardless.
            let _ = proxy_state::restore(&marker.snapshot);
        }
    }
    let _ = std::fs::remove_file(&path);
    // Strip the transient pid before handing the marker back to the
    // caller — they don't need it and it keeps the public type simple.
    marker.pid = 0;
    Ok(Some(marker))
}

pub fn clear(app_data_dir: &Path) {
    let _ = std::fs::remove_file(marker_path(app_data_dir));
}