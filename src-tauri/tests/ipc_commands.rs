use quietly_lib::ipc::commands::*;
use quietly_lib::storage::{block_rule, schedule, session, block_hit, settings, db};
use quietly_lib::storage::block_rule::BlockRule;
use quietly_lib::storage::schedule::Schedule;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

fn make_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let manager = SqliteConnectionManager::file(&path).with_init(|c| c.execute_batch("PRAGMA foreign_keys = ON;"));
    let pool = Pool::builder().max_size(2).build(manager).unwrap();
    db::init_schema_sync(&pool.get().unwrap()).unwrap();
    let state = AppState::new(pool, dir.path().to_path_buf());
    (state, dir)
}

#[test]
fn list_block_rules_empty() {
    let (s, _d) = make_state();
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 0);
}

#[test]
fn create_block_rule_returns_id() {
    let (s, _d) = make_state();
    let rule = BlockRule {
        id: None,
        kind: "url".into(),
        pattern: "twitter.com".into(),
        match_type: "contains".into(),
        note: "".into(),
        active: true,
        created_at: 0,
    };
    let id = create_block_rule_inner(&s, rule).unwrap();
    assert!(id > 0);
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 1);
}

#[test]
fn update_block_rule_round_trip() {
    let (s, _d) = make_state();
    let mut rule = BlockRule {
        id: None,
        kind: "url".into(),
        pattern: "twitter.com".into(),
        match_type: "contains".into(),
        note: "".into(),
        active: true,
        created_at: 0,
    };
    let id = create_block_rule_inner(&s, rule.clone()).unwrap();
    rule.id = Some(id);
    rule.note = "updated".into();
    update_block_rule_inner(&s, rule).unwrap();
    let got = list_block_rules_inner(&s).unwrap();
    assert_eq!(got[0].note, "updated");
}

#[test]
fn delete_block_rule() {
    let (s, _d) = make_state();
    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "url".into(), pattern: "x.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).unwrap();
    delete_block_rule_inner(&s, id).unwrap();
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 0);
}

#[test]
fn set_block_rule_active() {
    let (s, _d) = make_state();
    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "url".into(), pattern: "x.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).unwrap();
    set_block_rule_active_inner(&s, id, false).unwrap();
    assert!(!list_block_rules_inner(&s).unwrap()[0].active);
}

#[test]
fn list_schedules_empty() {
    let (s, _d) = make_state();
    assert_eq!(list_schedules_inner(&s).unwrap().len(), 0);
}

#[test]
fn create_schedule_returns_id() {
    let (s, _d) = make_state();
    let id = create_schedule_inner(&s, Schedule {
        id: None, name: "Work".into(), kind: "timer".into(),
        enabled: true, config_json: "{}".into(),
        block_rule_ids: "".into(), created_at: 0,
    }).unwrap();
    assert!(id > 0);
}

#[test]
fn toggle_schedule() {
    let (s, _d) = make_state();
    let id = create_schedule_inner(&s, Schedule {
        id: None, name: "W".into(), kind: "timer".into(),
        enabled: true, config_json: "{}".into(),
        block_rule_ids: "".into(), created_at: 0,
    }).unwrap();
    toggle_schedule_inner(&s, id, false).unwrap();
    let got = list_schedules_inner(&s).unwrap();
    assert!(!got[0].enabled);
}

#[test]
fn delete_schedule() {
    let (s, _d) = make_state();
    let id = create_schedule_inner(&s, Schedule {
        id: None, name: "W".into(), kind: "timer".into(),
        enabled: true, config_json: "{}".into(),
        block_rule_ids: "".into(), created_at: 0,
    }).unwrap();
    delete_schedule_inner(&s, id).unwrap();
    assert_eq!(list_schedules_inner(&s).unwrap().len(), 0);
}

#[test]
fn settings_default_empty() {
    let (s, _d) = make_state();
    let v = get_settings_inner(&s).unwrap();
    assert_eq!(v, serde_json::json!({}));
}

#[test]
fn settings_set_and_get() {
    let (s, _d) = make_state();
    set_settings_inner(&s, serde_json::json!({"proxy_port": 8888})).unwrap();
    let v = get_settings_inner(&s).unwrap();
    assert_eq!(v["proxy_port"], 8888);
}

#[test]
fn first_run_ack_default_true() {
    let (s, _d) = make_state();
    assert!(needs_first_run_ack_inner(&s).unwrap());
}

#[test]
fn first_run_ack_after_ack() {
    let (s, _d) = make_state();
    ack_first_run_inner(&s).unwrap();
    assert!(!needs_first_run_ack_inner(&s).unwrap());
}

#[test]
fn list_block_hits_empty() {
    let (s, _d) = make_state();
    assert_eq!(list_block_hits_inner(&s, 0, 100).unwrap().len(), 0);
}

#[test]
fn log_block_hit_then_list() {
    let (s, _d) = make_state();
    log_block_hit_inner(&s, "url".into(), "https://x.com".into(), None, None).unwrap();
    let hits = list_block_hits_inner(&s, 0, 100).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://x.com");
}

#[test]
fn list_sessions_empty() {
    let (s, _d) = make_state();
    assert_eq!(list_sessions_inner(&s, 0).unwrap().len(), 0);
}

#[test]
fn session_lifecycle_via_commands() {
    let (s, _d) = make_state();
    let id = session::start(&s.db, None, "focus", 1500).unwrap();
    session::end(&s.db, id, true).unwrap();
    let ss = list_sessions_inner(&s, 0).unwrap();
    assert_eq!(ss.len(), 1);
    assert_eq!(ss[0].phase, "focus");
}

#[test]
fn get_stats_returns_empty_when_no_sessions() {
    let (s, _d) = make_state();
    let v = get_stats_inner(&s, "today".into()).unwrap();
    assert!(v["sessions_today"].as_i64().unwrap() == 0);
}

#[test]
fn pause_resume_cancel_without_active_timer_errors() {
    let (s, _d) = make_state();
    assert!(pause_focus_inner(&s).is_err());
    assert!(resume_focus_inner(&s).is_err());
    assert!(cancel_focus_inner(&s).is_err());
}

#[test]
fn first_run_ack_is_idempotent() {
    let (s, _d) = make_state();
    assert!(needs_first_run_ack_inner(&s).unwrap());
    ack_first_run_inner(&s).unwrap();
    assert!(!needs_first_run_ack_inner(&s).unwrap());
    // Calling ack again must not error and must remain acked.
    ack_first_run_inner(&s).unwrap();
    assert!(!needs_first_run_ack_inner(&s).unwrap());
}

// `graceful_shutdown` is wired to both the window-close handler and the
// tray Quit action. It must be safe to call when no session is active —
// each step silently swallows "not running" errors so a half-initialized
// state can't leak a stuck proxy. This is the contract that makes
// double-clicking the exe (with no prior focus session) always safe.
#[tokio::test]
async fn graceful_shutdown_is_noop_on_idle_state() {
    let (s, _d) = make_state();
    // Should not panic, should not error — just return.
    graceful_shutdown(&s).await;
    // State should remain unchanged.
    assert!(s.active_timer.lock().unwrap().is_none());
    assert!(s.active_proxy.lock().unwrap().is_none());
    assert!(s.active_guard.lock().unwrap().is_none());
    assert!(s.proxy_snapshot.lock().unwrap().is_none());
}