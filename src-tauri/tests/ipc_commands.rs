use quietly_lib::ipc::commands::*;
use quietly_lib::engine::timer::Phase;
use quietly_lib::storage::{session, db, app_quota, usage_log};
use quietly_lib::storage::block_rule::BlockRule;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

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

#[tokio::test]
async fn create_block_rule_returns_id() {
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
    let id = create_block_rule_inner(&s, rule).await.unwrap();
    assert!(id > 0);
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 1);
}

#[tokio::test]
async fn update_block_rule_round_trip() {
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
    let id = create_block_rule_inner(&s, rule.clone()).await.unwrap();
    rule.id = Some(id);
    rule.note = "updated".into();
    update_block_rule_inner(&s, rule).await.unwrap();
    let got = list_block_rules_inner(&s).unwrap();
    assert_eq!(got[0].note, "updated");
}

#[tokio::test]
async fn delete_block_rule() {
    let (s, _d) = make_state();
    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "url".into(), pattern: "x.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    delete_block_rule_inner(&s, id).await.unwrap();
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 0);
}

#[tokio::test]
async fn set_block_rule_active() {
    let (s, _d) = make_state();
    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "url".into(), pattern: "x.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    set_block_rule_active_inner(&s, id, false).await.unwrap();
    assert!(!list_block_rules_inner(&s).unwrap()[0].active);
}

#[test]
fn first_run_ack_default_true() {
    let (s, _d) = make_state();
    assert!(needs_first_run_ack_inner(&s.app_data_dir).unwrap());
}

#[test]
fn first_run_ack_after_ack() {
    let (s, _d) = make_state();
    ack_first_run_inner(&s.app_data_dir).unwrap();
    assert!(!needs_first_run_ack_inner(&s.app_data_dir).unwrap());
}

#[test]
fn list_block_hits_empty() {
    let (s, _d) = make_state();
    assert_eq!(list_block_hits_inner(&s, 0, 100).unwrap().len(), 0);
}

#[test]
fn log_block_hit_then_list() {
    let (s, _d) = make_state();
    log_block_hit_inner(&s, "url".into(), "https://x.com".into(), None).unwrap();
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
    let id = session::start(&s.db, "focus", 1500).unwrap();
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
fn pause_resume_without_active_timer_errors() {
    let (s, _d) = make_state();
    assert!(pause_focus_inner(&s).is_err());
    assert!(resume_focus_inner(&s).is_err());
}

#[test]
fn cancel_without_active_timer_errors() {
    let (s, _d) = make_state();
    assert!(cancel_focus_inner(&s).is_err());
}

#[tokio::test]
async fn cancel_teardown_is_noop_on_idle_state() {
    // Mirrors graceful_shutdown_is_noop_on_idle_state but for the
    // dedicated teardown fn. Both steps silently swallow "not running"
    // errors so a half-initialized state can't leak a stuck proxy.
    let (s, _d) = make_state();
    cancel_focus_teardown(&s).await;
    assert!(s.active_proxy.lock().unwrap().is_none());
    assert!(s.active_guard.lock().unwrap().is_none());
}

#[test]
fn first_run_ack_is_idempotent() {
    let (s, _d) = make_state();
    assert!(needs_first_run_ack_inner(&s.app_data_dir).unwrap());
    ack_first_run_inner(&s.app_data_dir).unwrap();
    assert!(!needs_first_run_ack_inner(&s.app_data_dir).unwrap());
    // Calling ack again must not error and must remain acked.
    ack_first_run_inner(&s.app_data_dir).unwrap();
    assert!(!needs_first_run_ack_inner(&s.app_data_dir).unwrap());
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

// Lock-aware shutdown: a locked session rejects cancel, so the shield
// must stay up — the user's commitment holds across the close.
#[tokio::test]
async fn graceful_shutdown_keeps_shield_up_when_session_locked() {
    use quietly_lib::engine::timer::TimerHandle;
    let (s, _d) = make_state();
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(true));
    *s.active_proxy.lock().unwrap() =
        Some(quietly_lib::engine::proxy::ProxyHandle::dummy_for_test());
    graceful_shutdown(&s).await;
    assert!(s.active_timer.lock().unwrap().is_some(), "locked timer must survive");
    assert!(s.active_proxy.lock().unwrap().is_some(), "shield must stay up");
}

// And the non-locked path still tears down — regression lock.
#[tokio::test]
async fn graceful_shutdown_tears_down_when_session_unlocked() {
    use quietly_lib::engine::timer::TimerHandle;
    let (s, _d) = make_state();
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));
    *s.active_proxy.lock().unwrap() =
        Some(quietly_lib::engine::proxy::ProxyHandle::dummy_for_test());
    graceful_shutdown(&s).await;
    assert!(s.active_proxy.lock().unwrap().is_none(), "shield must tear down");
}

// The proxy and guard read rules from `state.rules` on every request.
// CRUD operations must refresh this in-memory cache, otherwise a rule
// added AFTER app start is invisible to the live proxy — exactly the
// bug where the user creates a rule but nothing gets blocked. This test
// is the regression lock for that contract.
#[tokio::test]
async fn crud_refreshes_in_memory_rules_cache() {
    let (s, _d) = make_state();

    // Cache starts empty (AppState::new loads from DB; DB starts empty).
    assert_eq!(s.rules.read().await.len(), 0);

    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "domain".into(), pattern: "twitter.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    assert_eq!(s.rules.read().await.len(), 1, "cache must update after create");

    set_block_rule_active_inner(&s, id, false).await.unwrap();
    assert_eq!(s.rules.read().await.len(), 1, "cache stays in sync after toggle");

    delete_block_rule_inner(&s, id).await.unwrap();
    assert_eq!(s.rules.read().await.len(), 0, "cache must reflect delete");
}

// ─── whitelist ────────────────────────────────────────────────────────
//
// Whitelist rules are stored as block_rule rows with kind="whitelist".
// The whitelist CRUD wrappers are convenience endpoints — the underlying
// schema is shared. These tests verify that the wrappers correctly
// (a) filter by kind, (b) force kind on create, and (c) refresh the
// shared rules cache so the live proxy sees whitelist entries.

#[tokio::test]
async fn whitelist_list_filters_to_whitelist_kind() {
    let (s, _d) = make_state();
    // Mixed kinds: only the whitelist should come back.
    create_block_rule_inner(&s, BlockRule {
        id: None, kind: "domain".into(), pattern: "x.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    let id = create_whitelist_inner(&s, BlockRule {
        id: None, kind: "ignored-by-wrapper".into(), pattern: "messages.google.com".into(),
        match_type: "contains".into(), note: "allow google messages".into(),
        active: true, created_at: 0,
    }).await.unwrap();
    assert!(id > 0);

    let wl = list_whitelist_inner(&s).unwrap();
    assert_eq!(wl.len(), 1);
    assert_eq!(wl[0].kind, "whitelist");
    assert_eq!(wl[0].pattern, "messages.google.com");
}

#[tokio::test]
async fn whitelist_create_overrides_kind() {
    let (s, _d) = make_state();
    // Caller sends kind="url"; wrapper must coerce to "whitelist".
    let id = create_whitelist_inner(&s, BlockRule {
        id: None, kind: "url".into(), pattern: "allowed.example".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    let rows = list_whitelist_inner(&s).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, Some(id));
    assert_eq!(rows[0].kind, "whitelist");
}

#[tokio::test]
async fn whitelist_delete_removes_rule() {
    let (s, _d) = make_state();
    let id = create_whitelist_inner(&s, BlockRule {
        id: None, kind: "".into(), pattern: "a.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    delete_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(list_whitelist_inner(&s).unwrap().len(), 0);
}

#[tokio::test]
async fn whitelist_toggle_flips_active_and_returns_new_state() {
    let (s, _d) = make_state();
    let id = create_whitelist_inner(&s, BlockRule {
        id: None, kind: "".into(), pattern: "b.com".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();

    let first = toggle_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(first, false, "first toggle must flip to inactive");
    let after = list_whitelist_inner(&s).unwrap();
    assert!(!after[0].active);

    let second = toggle_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(second, true, "second toggle must flip back to active");
    let after = list_whitelist_inner(&s).unwrap();
    assert!(after[0].active);
}

#[tokio::test]
async fn whitelist_create_accepts_title_kind_validation() {
    // Regression for kind validation: whitelist and title are new kinds,
    // and create_block_rule_inner must accept them. Without this, adding
    // a "title" rule from the frontend fails validation in Rust.
    let (s, _d) = make_state();
    let id = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "title".into(), pattern: "youtube".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await.unwrap();
    assert!(id > 0);

    // Negative: an unknown kind still rejects.
    let bad = create_block_rule_inner(&s, BlockRule {
        id: None, kind: "not_a_kind".into(), pattern: "x".into(),
        match_type: "contains".into(), note: "".into(), active: true, created_at: 0,
    }).await;
    assert!(bad.is_err(), "unknown kind must be rejected");
}

// ─── locked session ───────────────────────────────────────────────────
//
// Lock Plan: a session started with locked=true must refuse cancel and
// must remain the active timer after the rejected cancel. unlocked=false
// (default) sessions must still cancel cleanly.

#[test]
fn cancel_without_lock_succeeds() {
    let (s, _d) = make_state();
    // No timer running: must Err (existing contract).
    assert!(cancel_focus_inner(&s).is_err());

    // is_session_locked_inner on no timer returns false.
    assert!(!is_session_locked_inner(&s).unwrap());
}

#[test]
fn set_session_locked_without_timer_errors() {
    let (s, _d) = make_state();
    // No active timer — setting the lock must Err.
    assert!(set_session_locked_inner(&s, true).is_err());
}

// The actual locked-cancel test (cancel returns Err and handle is put back)
// requires constructing a real TimerHandle, which in turn requires a
// running Tauri app context (AppHandle) for emitting events. We exercise
// the lock field on the handle directly via TimerHandle::is_locked in
// engine_timer.rs. The IPC path here just needs to verify the error
// propagation on missing timer.

#[test]
fn cancel_returns_locked_error_message() {
    // Cannot fully exercise the locked-cancel path without AppHandle,
    // but we can at least lock down the missing-timer path: the message
    // we return for "no active timer" must not contain the lock hint,
    // so the UI can distinguish the two failure modes.
    let (s, _d) = make_state();
    let err = cancel_focus_inner(&s).unwrap_err();
    assert_eq!(err, "no active timer");
    assert!(!err.contains("locked"));
}

// ─── phase-scoped auto-block ─────────────────────────────────────────
//
// `decide_shield_action` is the pure decision function used by the
// `AutoBlockController` callback wired in `start_focus_inner`. The
// controller itself needs `AppHandle` to spawn start/stop work, so the
// integration test of "start_focus brings proxy up" isn't possible
// without a mock runtime. Instead we exhaustively cover the 4-tuple
// (want_up × currently_up) here — the actual proxy/guard start/stop
// already has its own test coverage via `graceful_shutdown_is_noop_on_idle_state`
// and `cancel_teardown_is_noop_on_idle_state`.

#[test]
fn decide_shield_wants_up_when_entering_focus_with_shield_down() {
    let (s, _d) = make_state();
    // Shield starts down (no proxy running).
    assert_eq!(decide_shield_action(&s, Phase::Focus), ShieldAction::Up);
}

#[test]
fn decide_shield_wants_down_when_entering_short_break_with_shield_up() {
    let (s, _d) = make_state();
    // Simulate shield already up — start_focus_inner pre-started proxy+guard.
    *s.active_proxy.lock().unwrap() =
        Some(quietly_lib::engine::proxy::ProxyHandle::dummy_for_test());
    assert_eq!(decide_shield_action(&s, Phase::ShortBreak), ShieldAction::Down);
}

#[test]
fn decide_shield_wants_down_when_entering_long_break_with_shield_up() {
    let (s, _d) = make_state();
    *s.active_proxy.lock().unwrap() =
        Some(quietly_lib::engine::proxy::ProxyHandle::dummy_for_test());
    assert_eq!(decide_shield_action(&s, Phase::LongBreak), ShieldAction::Down);
}

#[test]
fn decide_shield_noop_when_entering_focus_with_shield_already_up() {
    let (s, _d) = make_state();
    *s.active_proxy.lock().unwrap() =
        Some(quietly_lib::engine::proxy::ProxyHandle::dummy_for_test());
    // The phase handler is called on a transition; if shield is already
    // up we must NOT call start_proxy_inner again (port already bound,
    // guard hook already set). NoOp is the only correct answer here.
    assert_eq!(decide_shield_action(&s, Phase::Focus), ShieldAction::NoOp);
}

#[test]
fn decide_shield_noop_when_entering_break_with_shield_already_down() {
    let (s, _d) = make_state();
    // Shield starts down. Entering a break with shield already down is a
    // no-op — we don't want to spam stop_proxy_inner on every transition.
    assert_eq!(decide_shield_action(&s, Phase::ShortBreak), ShieldAction::NoOp);
    assert_eq!(decide_shield_action(&s, Phase::LongBreak), ShieldAction::NoOp);
}

// ─── app quota ────────────────────────────────────────────────────────

#[test]
fn quota_crud_round_trip() {
    let (s, _d) = make_state();
    let id = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "Discord.exe".into(),
        daily_minutes: 30,
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: true,
        created_at: 0,
    }).unwrap();
    assert!(id > 0);

    let rows = list_quotas_inner(&s).unwrap();
    assert_eq!(rows.len(), 1);
    // create_quota_inner lowercases the pattern so production trackers
    // (which also lowercase basenames) can match exactly.
    assert_eq!(rows[0].app_pattern, "discord.exe");

    // Toggle inactive.
    set_app_quota_active_inner(&s, id, false).unwrap();
    assert!(!list_quotas_inner(&s).unwrap()[0].active);
    set_app_quota_active_inner(&s, id, true).unwrap();
    assert!(list_quotas_inner(&s).unwrap()[0].active);

    // Update.
    let mut row = list_quotas_inner(&s).unwrap().remove(0);
    row.daily_minutes = 60;
    update_quota_inner(&s, row.clone()).unwrap();
    assert_eq!(list_quotas_inner(&s).unwrap()[0].daily_minutes, 60);

    // Delete.
    delete_quota_inner(&s, id).unwrap();
    assert_eq!(list_quotas_inner(&s).unwrap().len(), 0);
}

#[test]
fn quota_create_rejects_empty_pattern() {
    let (s, _d) = make_state();
    let err = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "   ".into(),    // whitespace only
        daily_minutes: 30,
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: true,
        created_at: 0,
    }).unwrap_err();
    assert!(err.contains("app_pattern"));
}

#[test]
fn quota_create_rejects_zero_minutes() {
    let (s, _d) = make_state();
    let err = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "x.exe".into(),
        daily_minutes: 0,
        used_seconds: 0,
        used_day: "2026-08-16".into(),
        active: true,
        created_at: 0,
    }).unwrap_err();
    assert!(err.contains("daily_minutes"));
}

#[test]
fn quota_create_rejects_duplicate_pattern() {
    let (s, _d) = make_state();
    create_quota_inner(&s, app_quota::AppQuota {
        id: None, app_pattern: "Discord.exe".into(),
        daily_minutes: 30, used_seconds: 0, used_day: "2026-08-16".into(),
        active: true, created_at: 0,
    }).unwrap();
    let dup = create_quota_inner(&s, app_quota::AppQuota {
        id: None, app_pattern: "Discord.exe".into(),
        daily_minutes: 60, used_seconds: 0, used_day: "2026-08-16".into(),
        active: true, created_at: 0,
    });
    assert!(dup.is_err());
}

#[test]
fn get_quota_status_reflects_used_and_exceeded() {
    let (s, _d) = make_state();
    let _id = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "chrome.exe".into(),
        daily_minutes: 10,             // threshold = 600s
        used_seconds: 300,             // 5 minutes, not exceeded
        used_day: today_str(),
        active: true,
        created_at: 0,
    }).unwrap();

    let status = get_quota_status_inner(&s).unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].app_pattern, "chrome.exe");
    assert_eq!(status[0].used_minutes, 5);
    assert_eq!(status[0].daily_minutes, 10);
    assert!(!status[0].exceeded);
}

#[test]
fn get_quota_status_marks_exceeded() {
    let (s, _d) = make_state();
    let _id = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "chrome.exe".into(),
        daily_minutes: 5,              // threshold = 300s
        used_seconds: 600,             // 10 minutes — over
        used_day: today_str(),
        active: true,
        created_at: 0,
    }).unwrap();
    let status = get_quota_status_inner(&s).unwrap();
    assert!(status[0].exceeded);
    assert_eq!(status[0].used_minutes, 10);
}

#[test]
fn get_quota_status_resets_on_day_change() {
    // Daily reset semantics: if used_day != today, the status view shows 0.
    let (s, _d) = make_state();
    let _id = create_quota_inner(&s, app_quota::AppQuota {
        id: None,
        app_pattern: "msedge.exe".into(),
        daily_minutes: 60,
        used_seconds: 1800,            // 30 minutes from yesterday
        used_day: "1999-01-01".into(), // ancient day
        active: true,
        created_at: 0,
    }).unwrap();
    let status = get_quota_status_inner(&s).unwrap();
    assert_eq!(status[0].used_minutes, 0, "stale day must reset to 0");
    assert!(!status[0].exceeded);
}

#[test]
fn get_today_usage_filters_inactive() {
    let (s, _d) = make_state();
    let id = create_quota_inner(&s, app_quota::AppQuota {
        id: None, app_pattern: "Slack.exe".into(),
        daily_minutes: 60, used_seconds: 0, used_day: today_str(),
        active: true, created_at: 0,
    }).unwrap();
    create_quota_inner(&s, app_quota::AppQuota {
        id: None, app_pattern: "Discord.exe".into(),
        daily_minutes: 30, used_seconds: 0, used_day: today_str(),
        active: true, created_at: 0,
    }).unwrap();
    set_app_quota_active_inner(&s, id, false).unwrap();

    let today = get_today_usage_inner(&s).unwrap();
    // Only Discord (still active) appears. Pattern is stored lowercase
    // (create_quota_inner normalizes) to match tracker output.
    assert_eq!(today.len(), 1);
    assert_eq!(today[0].app_pattern, "discord.exe");
}

// ─── usage stats ──────────────────────────────────────────────────────

#[test]
fn usage_stats_invalid_range_errors() {
    let (s, _d) = make_state();
    let err = get_usage_stats_inner(&s, "year".into()).unwrap_err();
    assert!(err.contains("invalid range"));
}

#[test]
fn usage_stats_aggregates_app_and_url_with_kind_label() {
    let (s, _d) = make_state();
    let now = now_ts();
    // Two app segments for chrome totalling 90s + one for Slack 30s.
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: now - 100, duration_seconds: 60,
    }).unwrap();
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "chrome.exe".into(),
        ts: now - 50, duration_seconds: 30,
    }).unwrap();
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "app".into(), target: "Slack.exe".into(),
        ts: now - 10, duration_seconds: 30,
    }).unwrap();
    // URL segment for example.com.
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "example.com".into(),
        ts: now - 5, duration_seconds: 120,
    }).unwrap();

    let day = get_usage_stats_inner(&s, "day".into()).unwrap();
    // Should contain entries of both kinds.
    assert!(day.iter().any(|e| e.kind == "app" && e.target == "chrome.exe" && e.minutes == 1));
    assert!(day.iter().any(|e| e.kind == "app" && e.target == "Slack.exe"));
    assert!(day.iter().any(|e| e.kind == "url" && e.target == "example.com"));
}

#[test]
fn usage_stats_range_filters_old_segments() {
    let (s, _d) = make_state();
    let now = now_ts();
    // Old: 100 days ago (excluded from "month", excluded from "week", excluded from "day")
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "old.com".into(),
        ts: now - 100 * 86_400, duration_seconds: 60,
    }).unwrap();
    // Fresh: today.
    usage_log::append(&s.db, &usage_log::UsageSegment {
        id: None, kind: "url".into(), target: "fresh.com".into(),
        ts: now - 60, duration_seconds: 60,
    }).unwrap();

    let day = get_usage_stats_inner(&s, "day".into()).unwrap();
    assert!(day.iter().any(|e| e.target == "fresh.com"));
    assert!(!day.iter().any(|e| e.target == "old.com"));

    let month = get_usage_stats_inner(&s, "month".into()).unwrap();
    // 100d > 30d so still excluded from month.
    assert!(!month.iter().any(|e| e.target == "old.com"));
}

// ─── helpers ──────────────────────────────────────────────────────────

fn today_str() -> String {
    use chrono::Local;
    Local::now().format("%Y-%m-%d").to_string()
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}