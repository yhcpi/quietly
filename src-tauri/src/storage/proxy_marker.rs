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

/// Restore the proxy state recorded in the marker (if any), then delete the
/// marker. Intended to be called once on app startup. Returns:
/// - Ok(Some(marker)) if a stale marker was found and restored
/// - Ok(None) if no marker existed (clean state)
pub fn reap(app_data_dir: &Path) -> std::io::Result<Option<ProxyMarker>> {
    let path = marker_path(app_data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let marker: ProxyMarker = match serde_json::from_slice(&bytes) {
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
    Ok(Some(marker))
}

pub fn clear(app_data_dir: &Path) {
    let _ = std::fs::remove_file(marker_path(app_data_dir));
}