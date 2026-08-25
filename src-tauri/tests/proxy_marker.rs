use quietly_lib::engine::proxy_state::ProxySnapshot;
use quietly_lib::storage::proxy_marker::{self, ProxyMarker};

fn marker_with(port: u16) -> ProxyMarker {
    ProxyMarker {
        port,
        pid: 0,
        snapshot: ProxySnapshot {
            enable: false,
            server: None,
            override_str: None,
        },
    }
}

#[test]
fn write_creates_marker_file() {
    let dir = tempfile::tempdir().unwrap();
    proxy_marker::write(dir.path(), &marker_with(8080)).unwrap();
    assert!(dir.path().join("proxy-marker.json").exists());
}

#[test]
fn clear_removes_marker_file() {
    let dir = tempfile::tempdir().unwrap();
    proxy_marker::write(dir.path(), &marker_with(8080)).unwrap();
    proxy_marker::clear(dir.path());
    assert!(!dir.path().join("proxy-marker.json").exists());
}

#[test]
fn clear_on_missing_marker_is_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    // Must not panic / error — used in the stop_proxy path which runs
    // even when no marker was ever written.
    proxy_marker::clear(dir.path());
}

#[test]
fn reap_returns_none_when_no_marker() {
    let dir = tempfile::tempdir().unwrap();
    let res = proxy_marker::reap(dir.path()).unwrap();
    assert!(res.is_none());
}

#[test]
fn reap_deletes_unparseable_marker_and_returns_err() {
    let dir = tempfile::tempdir().unwrap();
    // Write garbage so serde_json::from_slice fails. reap must remove the
    // file (otherwise the next launch will keep failing forever) and
    // surface an InvalidData error.
    std::fs::write(dir.path().join("proxy-marker.json"), b"not json {{{").unwrap();
    let err = proxy_marker::reap(dir.path()).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(!dir.path().join("proxy-marker.json").exists());
}