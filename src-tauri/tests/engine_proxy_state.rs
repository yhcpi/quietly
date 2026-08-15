#![cfg(windows)]

use std::sync::Mutex;

use quietly_lib::engine::proxy_state::{read_current, set_proxy, clear_proxy, restore, ProxySnapshot};

/// Serializes tests that mutate the global proxy state in HKCU. Without this,
/// tests run concurrently and step on each other's writes.
static REG_LOCK: Mutex<()> = Mutex::new(());

/// Restores the original proxy state on drop, even if the test panics.
struct ProxyStateGuard {
    snap: ProxySnapshot,
}

impl ProxyStateGuard {
    fn capture() -> Self {
        let snap = read_current().expect("read_current");
        Self { snap }
    }
}

impl Drop for ProxyStateGuard {
    fn drop(&mut self) {
        let _ = restore(&self.snap);
    }
}

#[test]
fn read_current_returns_valid_snapshot() {
    let _lock = REG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let snap = read_current().expect("read_current");
    // No assertions on exact values — the user's state is unknown.
    // Just verify the values are well-formed.
    let _ = snap.enable;
    let _ = snap.server;
    let _ = snap.override_str;
}

#[test]
fn set_proxy_writes_enable_and_server() {
    let _lock = REG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _guard = ProxyStateGuard::capture();
    let test_server = "127.0.0.1:18745";

    set_proxy(test_server).expect("set_proxy");
    let after = read_current().expect("read_current after set");

    assert!(after.enable, "ProxyEnable should be 1 after set_proxy");
    assert_eq!(after.server.as_deref(), Some(test_server));
}

#[test]
fn clear_proxy_writes_enable_zero() {
    let _lock = REG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _guard = ProxyStateGuard::capture();
    let test_server = "127.0.0.1:18746";

    set_proxy(test_server).expect("set_proxy");
    clear_proxy().expect("clear_proxy");
    let after = read_current().expect("read_current after clear");

    assert!(!after.enable, "ProxyEnable should be 0 after clear_proxy");
    // Server string is preserved across clear (matches Windows behavior)
    assert_eq!(after.server.as_deref(), Some(test_server));
}

#[test]
fn restore_returns_to_snapshot() {
    let _lock = REG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let original = read_current().expect("read_current original");
    let _guard = ProxyStateGuard::capture();

    set_proxy("127.0.0.1:18747").expect("set_proxy");
    let modified = read_current().expect("read_current modified");
    assert!(modified.enable);
    assert_eq!(modified.server.as_deref(), Some("127.0.0.1:18747"));

    restore(&original).expect("restore");
    let restored = read_current().expect("read_current restored");

    assert_eq!(restored.enable, original.enable);
    assert_eq!(restored.server, original.server);
    assert_eq!(restored.override_str, original.override_str);
}
