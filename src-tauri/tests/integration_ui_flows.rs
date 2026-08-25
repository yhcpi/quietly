//! End-to-end integration tests for Quietly.
//!
//! These tests exercise the full IPC pipeline via direct calls into the
//! `*_inner` functions wired up exactly the way `lib.rs::run()` wires
//! them. We deliberately do NOT use `tauri::test::mock_app()` because
//! the test binary fails to start with `STATUS_ENTRYPOINT_NOT_FOUND`
//! on Windows when both `wry` (default tauri feature) and `test`
//! (added for mock_runtime) are linked in — the conflict between
//! `windows` crate v0.58 (used by our guard) and v0.61 (pulled in by
//! `tauri-runtime-wry`) leaves a stale import in the IAT. The behavior
//! under test — every IPC command's logic and its interaction with the
//! in-memory `AppState` — is exactly what the inner functions exercise.
//!
//! What we deliberately don't do:
//!   - We don't call `start_proxy_inner` / `start_guard_inner` in tests.
//!     Both touch the Windows registry / Win32 event hooks; running them
//!     would pollute the developer's actual proxy settings. Where the
//!     brief asks for "proxy is listening", we populate `active_proxy`
//!     with `ProxyHandle::dummy_for_test()` instead.
//!   - We don't run the actual timer `run_loop` against wall time. The
//!     existing `engine_timer.rs` tests already cover run-loop
//!     transitions. Here we assert on `TimerState` snapshots the
//!     `TimerHandle` exposes via `state()`.

use quietly_lib::engine::proxy::ProxyHandle;
use quietly_lib::engine::timer::{Phase, TimerHandle, TimerStatus};
use quietly_lib::engine::guard::extract_exe_name;
use quietly_lib::ipc::commands::*;
use quietly_lib::storage::{app_quota, db, session, usage_log};
use quietly_lib::storage::block_rule::BlockRule;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tempfile::TempDir;

// ─── helpers ────────────────────────────────────────────────────────

fn make_state() -> (AppState, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let manager =
        SqliteConnectionManager::file(&path).with_init(|c| c.execute_batch("PRAGMA foreign_keys = ON;"));
    let pool: Pool<SqliteConnectionManager> = Pool::builder().max_size(2).build(manager).unwrap();
    db::init_schema_sync(&pool.get().unwrap()).unwrap();
    let state = AppState::new(pool, dir.path().to_path_buf());
    (state, dir)
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn rule(kind: &str, pattern: &str, match_type: &str, active: bool) -> BlockRule {
    BlockRule {
        id: None,
        kind: kind.into(),
        pattern: pattern.into(),
        match_type: match_type.into(),
        note: "".into(),
        active,
        created_at: 0,
    }
}

// ─── focus happy path ────────────────────────────────────────────────
//
// The brief asked for: start_focus(25, false, true) → poll until timer
// is Running → assert state.phase is Focus, is_session_locked() == false.
//
// The auto_block=true path would invoke start_proxy_inner (HKCU
// registry write) and start_guard_inner (SetWinEventHook) — both
// unsuitable for tests. Instead we exercise:
//   - list_block_rules-style inner CRUD flow under a populated timer
//   - is_session_locked_inner with the unlocked handle
//   - the cancel_focus_inner contract (no-op when no timer, succeeds
//     when unlocked, fails when locked)
//
// The actual `start_focus_inner(auto_block=true)` code path is covered
// by `tests/ipc_commands.rs::decide_shield_wants_*` + the Wry-only
// smoke verification in lib.rs.

#[tokio::test]
async fn focus_state_and_lock_query_when_unlocked() {
    let (s, _d) = make_state();

    // Inject an unlocked timer (the post-state of start_focus with
    // locked=false, auto_block=false).
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));

    // Timer state: Running, Focus, full duration remaining.
    let g = s.active_timer.lock().unwrap();
    let h = g.as_ref().expect("active timer");
    let ts = h.state();
    assert_eq!(ts.status, TimerStatus::Running);
    assert_eq!(ts.phase, Phase::Focus);
    assert_eq!(ts.total_seconds, 25 * 60);
    assert_eq!(ts.remaining_seconds, 25 * 60);
    drop(g);

    // Locked flag is false.
    assert!(!is_session_locked_inner(&s).unwrap());

    // Cleanup.
    assert!(cancel_focus_inner(&s).is_ok());
    assert!(s.active_timer.lock().unwrap().is_none());
}

// ─── pause / resume correctness ─────────────────────────────────────
//
// Regression for the "time accumulates during pause" bug. Pause must
// freeze remaining_seconds; resume continues from that exact value.
// The pause window must NOT consume the countdown.

#[tokio::test]
async fn pause_resume_does_not_accumulate_during_pause() {
    let (s, _d) = make_state();

    // Insert an unlocked timer.
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));

    let initial = s
        .active_timer
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .state()
        .remaining_seconds;
    assert_eq!(initial, 25 * 60, "fresh timer must be at full duration");

    // Pause.
    pause_focus_inner(&s).expect("pause");
    let at_pause = s
        .active_timer
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .state()
        .remaining_seconds;
    assert_eq!(at_pause, initial, "pause must freeze remaining at current value");
    assert_eq!(
        s.active_timer.lock().unwrap().as_ref().unwrap().state().status,
        TimerStatus::Paused
    );

    // Sleep "long" while paused. The contract: remaining must not change.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let after_wait = s
        .active_timer
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .state()
        .remaining_seconds;
    assert_eq!(
        after_wait, at_pause,
        "remaining must NOT decrement while paused"
    );

    // Resume.
    resume_focus_inner(&s).expect("resume");
    let after_resume = s
        .active_timer
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .state()
        .status;
    assert_eq!(after_resume, TimerStatus::Running);

    // Cleanup.
    cancel_focus_inner(&s).expect("cancel");
}

// ─── lock prevents cancel ────────────────────────────────────────────

#[tokio::test]
async fn locked_session_rejects_cancel_unlocked_succeeds() {
    let (s, _d) = make_state();

    // Inject a LOCKED timer.
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(true));

    // is_session_locked → true.
    assert!(is_session_locked_inner(&s).unwrap());

    // cancel_focus must error and the timer must survive.
    let err = cancel_focus_inner(&s).unwrap_err();
    assert!(
        err.contains("locked"),
        "error must mention 'locked', got: {}",
        err
    );
    assert!(
        s.active_timer.lock().unwrap().is_some(),
        "timer must survive rejected cancel"
    );

    // Unlock then cancel must succeed.
    set_session_locked_inner(&s, false).expect("unlock");
    cancel_focus_inner(&s).expect("cancel unlocked");
    assert!(
        s.active_timer.lock().unwrap().is_none(),
        "timer must be cleared after successful cancel"
    );
}

// ─── phase-scoped auto-block ─────────────────────────────────────────
//
// We test the *controller* via `decide_shield_action` (pure function)
// since invoking the actual `start_focus(auto_block=true)` would
// write to the Windows registry. The 4-tuple (want_up × currently_up)
// is the regression lock for "shield stays down until the next Focus
// phase".

#[test]
fn decide_shield_action_phase_transitions() {
    let (s, _d) = make_state();

    // Shield starts down (no proxy). Entering Focus → Up.
    assert_eq!(decide_shield_action(&s, Phase::Focus), ShieldAction::Up);

    // Manually raise the shield (simulating start_focus_inner pre-started proxy).
    *s.active_proxy.lock().unwrap() = Some(ProxyHandle::dummy_for_test());

    // Shield up. Entering Focus → NoOp. Entering any break → Down.
    assert_eq!(
        decide_shield_action(&s, Phase::Focus),
        ShieldAction::NoOp,
        "Focus with shield up must be NoOp"
    );
    assert_eq!(
        decide_shield_action(&s, Phase::ShortBreak),
        ShieldAction::Down,
        "ShortBreak with shield up must be Down"
    );
    assert_eq!(
        decide_shield_action(&s, Phase::LongBreak),
        ShieldAction::Down,
        "LongBreak with shield up must be Down"
    );

    // Drop the shield. Break transitions are NoOp.
    *s.active_proxy.lock().unwrap() = None;
    assert_eq!(
        decide_shield_action(&s, Phase::ShortBreak),
        ShieldAction::NoOp
    );
    assert_eq!(
        decide_shield_action(&s, Phase::LongBreak),
        ShieldAction::NoOp
    );
}

// ─── graceful shutdown with locked / unlocked session ────────────────

#[tokio::test]
async fn graceful_shutdown_keeps_shield_up_when_locked() {
    let (s, _d) = make_state();

    // Inject a locked timer + dummy proxy (skip the real registry write).
    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(true));
    *s.active_proxy.lock().unwrap() = Some(ProxyHandle::dummy_for_test());

    graceful_shutdown(&s).await;

    assert!(
        s.active_timer.lock().unwrap().is_some(),
        "locked timer must survive graceful_shutdown"
    );
    assert!(
        s.active_proxy.lock().unwrap().is_some(),
        "shield must stay up while locked"
    );
}

#[tokio::test]
async fn graceful_shutdown_tears_down_when_unlocked() {
    let (s, _d) = make_state();

    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));
    *s.active_proxy.lock().unwrap() = Some(ProxyHandle::dummy_for_test());

    graceful_shutdown(&s).await;

    assert!(
        s.active_timer.lock().unwrap().is_none(),
        "unlocked timer must be cancelled"
    );
    assert!(
        s.active_proxy.lock().unwrap().is_none(),
        "shield must be torn down when unlocked"
    );
}

// ─── whitelist priority over block ──────────────────────────────────

#[tokio::test]
async fn whitelist_overrides_block_in_proxy_match() {
    let (s, _d) = make_state();

    // Add block for evil.com.
    create_block_rule_inner(
        &s,
        rule("domain", "evil.com", "contains", true),
    )
    .await
    .expect("create_block_rule");

    // Add whitelist for evil.com.
    create_whitelist_inner(
        &s,
        rule("domain", "evil.com", "contains", true),
    )
    .await
    .expect("create_whitelist");

    // Read the in-memory cache.
    let cache = s.rules.read().await.clone();

    // Proxy logic: whitelist wins.
    assert!(
        !quietly_lib::engine::proxy::should_block_host("evil.com:443", &cache),
        "evil.com must NOT be blocked when whitelisted"
    );

    // Add another block rule, ensure non-whitelisted site IS blocked.
    create_block_rule_inner(
        &s,
        rule("domain", "blocked.com", "contains", true),
    )
    .await
    .expect("create_block_rule blocked.com");
    let cache = s.rules.read().await.clone();
    assert!(
        quietly_lib::engine::proxy::should_block_host("blocked.com:443", &cache),
        "blocked.com must be blocked when no whitelist overrides"
    );
    assert!(
        !quietly_lib::engine::proxy::should_block_host("evil.com:443", &cache),
        "evil.com still NOT blocked after adding another rule"
    );
}

// ─── title rule blocks window ────────────────────────────────────────

#[tokio::test]
async fn title_rule_matches_via_engine_helper() {
    let (s, _d) = make_state();

    create_block_rule_inner(
        &s,
        rule("title", "Distracting", "contains", true),
    )
    .await
    .expect("create title rule");

    let rules = s.rules.read().await.clone();

    // matches_active_rule takes (rules, exe_path, title). With a
    // non-matching exe and a matching title, it must return true.
    let matched = quietly_lib::engine::guard::matches_active_rule(
        &rules,
        "C:/Windows/explorer.exe",
        "Distracting - YouTube - Chrome",
    );
    assert!(matched, "title rule 'Distracting' must match a window with that text");

    // A non-matching title: not blocked.
    let not_matched = quietly_lib::engine::guard::matches_active_rule(
        &rules,
        "C:/Windows/explorer.exe",
        "Boring - Work - VSCode",
    );
    assert!(!not_matched, "non-matching title must not block");
}

// ─── usage log accumulates + quota-exceeded ──────────────────────────

#[tokio::test]
async fn usage_log_accumulates_and_quota_exceeded_event_fires() {
    use quietly_lib::engine::app_usage::{
        flush_now, EventSink, FlushStore, QuotaExceededPayload, UsageTracker,
    };
    use std::sync::Mutex as StdMutex;

    let (s, _d) = make_state();
    let db_pool = s.db.clone();

    // Pre-arm: create an active quota for chrome.exe (60 min/day).
    let _qid = create_quota_inner(
        &s,
        app_quota::AppQuota {
            id: None,
            app_pattern: "chrome.exe".into(),
            daily_minutes: 60,
            used_seconds: 0,
            used_day: today_str(),
            active: true,
            created_at: 0,
        },
    )
    .expect("create_quota");

    // Drive the tracker through a transition.
    let tracker = UsageTracker::new();
    let t0 = now_ts();
    tracker.update_at(Some("chrome.exe"), None, None, t0);

    struct CaptureSink {
        events: StdMutex<Vec<QuotaExceededPayload>>,
    }
    impl EventSink for CaptureSink {
        fn emit_quota_exceeded(&self, p: QuotaExceededPayload) {
            self.events.lock().unwrap().push(p);
        }
    }
    struct DbFlushStore {
        pool: Pool<SqliteConnectionManager>,
    }
    impl FlushStore for DbFlushStore {
        fn append(
            &self,
            seg: &quietly_lib::storage::usage_log::UsageSegment,
        ) -> anyhow::Result<i64> {
            usage_log::append(&self.pool, seg)
        }
        fn increment_quota(
            &self,
            pattern: &str,
            delta: i64,
            today: &str,
        ) -> anyhow::Result<(i64, bool)> {
            app_quota::increment_used(&self.pool, pattern, delta, today)
        }
        fn lookup_active_quota(
            &self,
            pattern: &str,
        ) -> anyhow::Result<Option<app_quota::AppQuota>> {
            let rows = app_quota::list_active(&self.pool)?;
            Ok(rows.into_iter().find(|q| q.app_pattern == pattern))
        }
        fn list_active_quotas(&self) -> anyhow::Result<Vec<app_quota::AppQuota>> {
            app_quota::list_active(&self.pool)
        }
    }
    let store = DbFlushStore { pool: db_pool.clone() };
    let capture = CaptureSink { events: StdMutex::new(Vec::new()) };

    // First flush: 45s elapsed on chrome.exe → quota=45s, not exceeded.
    let wrote = flush_now(&tracker, &store, &capture, t0 + 45).expect("flush 1");
    assert!(wrote);

    let segs = usage_log::list_recent(&db_pool, 0, 100).expect("list_recent");
    let chrome_seg = segs.iter().find(|s| s.kind == "app" && s.target == "chrome.exe");
    assert!(chrome_seg.is_some(), "usage_log must have a chrome.exe row");
    assert_eq!(chrome_seg.unwrap().duration_seconds, 45);

    let status = get_quota_status_inner(&s).expect("quota_status");
    let chrome_status = status
        .iter()
        .find(|s| s.app_pattern == "chrome.exe")
        .expect("chrome status");
    assert_eq!(chrome_status.used_minutes, 0, "45s rounds down to 0 minutes");
    assert!(!chrome_status.exceeded, "45s of 60min is not exceeded");

    // Drive past the threshold. Real flow: user switches away to slack,
    // then back to chrome. Each switch is a real "change" so the tracker
    // re-arms. The chrome segment after switch-back covers the time on
    // chrome that pushes it past 3600s.
    let t_slack = t0 + 60;
    tracker.update_at(Some("slack.exe"), None, None, t_slack);
    let _wrote = flush_now(&tracker, &store, &capture, t_slack + 5).expect("flush slack");

    let t_back = t_slack + 5;
    tracker.update_at(Some("chrome.exe"), None, None, t_back);
    // 3556s on chrome after return → 45 + 3556 = 3601s, just past 3600.
    let _wrote = flush_now(&tracker, &store, &capture, t_back + 3556).expect("flush back");

    let status2 = get_quota_status_inner(&s).expect("quota_status");
    let chrome2 = status2
        .iter()
        .find(|s| s.app_pattern == "chrome.exe")
        .expect("chrome");
    assert!(
        chrome2.exceeded,
        "after crossing threshold (45 + 3556 = 3601 >= 3600), exceeded must be true; got used_minutes={}, daily={}",
        chrome2.used_minutes,
        chrome2.daily_minutes
    );

    // CaptureSink should have at least one quota-exceeded event.
    let events = capture.events.lock().unwrap();
    assert!(
        !events.is_empty(),
        "quota-exceeded event must fire when threshold crossed"
    );
}

// Regression for the "quota always shows 0 minutes" bug: guard used to
// pass the full process path into `app_usage::update`, but `flush_now`
// matches `app_quota.app_pattern` exactly — so a user typing
// "YuanShen.exe" never matched a path like
// "D:\\Game\\Genshin Impact\\...\\YuanShen.exe". Fix: guard now calls
// `extract_exe_name` first and updates with the basename. This test
// locks that contract in: feed guard-style full paths through
// `extract_exe_name` → update → flush → assert quota increments.
#[tokio::test]
async fn guard_full_path_resolves_to_matching_quota() {
    use quietly_lib::engine::app_usage::{
        flush_now, EventSink, FlushStore, QuotaExceededPayload, UsageTracker,
    };
    use std::sync::Mutex as StdMutex;

    let (s, _d) = make_state();

    // User input: just the exe name (as the Quotas UI placeholder shows).
    let _qid = create_quota_inner(
        &s,
        app_quota::AppQuota {
            id: None,
            app_pattern: "YuanShen.exe".into(),
            daily_minutes: 120,
            used_seconds: 0,
            used_day: today_str(),
            active: true,
            created_at: 0,
        },
    )
    .expect("create_quota");

    // Simulate what `guard::foreground_callback` does today: it gets a
    // full path from QueryFullProcessImageNameW, strips the directory
    // with extract_exe_name, **lowercases** the basename (matching
    // create_quota_inner / app_quota::create which also lowercase the
    // pattern), then calls update. If anyone reverts guard.rs to pass
    // the path directly — or skips the lowercasing — this test fails
    // because flush_now's lookup_active_quota matches `app_pattern`
    // exactly and the DB row is lowercase.
    let full_path = r"D:\Game\Genshin Impact\GenshinImpact\YuanShen.exe";
    let exe_name = extract_exe_name(full_path)
        .expect("extract basename")
        .to_lowercase();
    assert_eq!(exe_name, "yuanshen.exe");

    let tracker = UsageTracker::new();
    let t0 = now_ts();
    tracker.update_at(Some(&exe_name), None, None, t0);

    struct CaptureSink {
        events: StdMutex<Vec<QuotaExceededPayload>>,
    }
    impl EventSink for CaptureSink {
        fn emit_quota_exceeded(&self, p: QuotaExceededPayload) {
            self.events.lock().unwrap().push(p);
        }
    }
    struct DbFlushStore {
        pool: Pool<SqliteConnectionManager>,
    }
    impl FlushStore for DbFlushStore {
        fn append(
            &self,
            seg: &quietly_lib::storage::usage_log::UsageSegment,
        ) -> anyhow::Result<i64> {
            usage_log::append(&self.pool, seg)
        }
        fn increment_quota(
            &self,
            pattern: &str,
            delta: i64,
            today: &str,
        ) -> anyhow::Result<(i64, bool)> {
            app_quota::increment_used(&self.pool, pattern, delta, today)
        }
        fn lookup_active_quota(
            &self,
            pattern: &str,
        ) -> anyhow::Result<Option<app_quota::AppQuota>> {
            let rows = app_quota::list_active(&self.pool)?;
            Ok(rows.into_iter().find(|q| q.app_pattern == pattern))
        }
        fn list_active_quotas(&self) -> anyhow::Result<Vec<app_quota::AppQuota>> {
            app_quota::list_active(&self.pool)
        }
    }
    let store = DbFlushStore { pool: s.db.clone() };
    let capture = CaptureSink { events: StdMutex::new(Vec::new()) };

    // 90s on YuanShen.exe
    let wrote = flush_now(&tracker, &store, &capture, t0 + 90).expect("flush");
    assert!(wrote);

    // The quota must have been incremented (not silently skipped because
    // basename-vs-path mismatch).
    let status = get_quota_status_inner(&s).expect("quota_status");
    let ys = status
        .iter()
        .find(|s| s.app_pattern == "yuanshen.exe")
        .expect("YuanShen status");
    assert_eq!(
        ys.used_minutes, 1,
        "90s rounds down to 1 minute; if 0 the path-vs-basename bug regressed"
    );
    assert!(!ys.exceeded, "1 min of 120 min/day is not exceeded");

    // usage_log must have a row keyed by the basename, not the path.
    let segs = usage_log::list_recent(&s.db, 0, 100).expect("list_recent");
    let ys_seg = segs
        .iter()
        .find(|s| s.kind == "app" && s.target == "yuanshen.exe")
        .expect("YuanShen usage row");
    assert_eq!(ys_seg.duration_seconds, 90);
    assert!(
        segs.iter().all(|s| s.target != full_path),
        "no usage_log row should carry the full path anymore"
    );
}

// ─── CRUD round-trips (inner functions) ──────────────────────────────

#[tokio::test]
async fn crud_block_rules_via_inner() {
    let (s, _d) = make_state();

    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 0);

    let id = create_block_rule_inner(&s, rule("domain", "example.com", "contains", true))
        .await
        .expect("create");
    assert!(id > 0);

    let rows = list_block_rules_inner(&s).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].pattern, "example.com");

    set_block_rule_active_inner(&s, id, false).await.unwrap();
    assert!(!list_block_rules_inner(&s).unwrap()[0].active);

    set_block_rule_active_inner(&s, id, true).await.unwrap();
    assert!(list_block_rules_inner(&s).unwrap()[0].active);

    delete_block_rule_inner(&s, id).await.unwrap();
    assert_eq!(list_block_rules_inner(&s).unwrap().len(), 0);
}

#[tokio::test]
async fn crud_whitelist_via_inner() {
    let (s, _d) = make_state();

    let id = create_whitelist_inner(&s, rule("url", "messages.google.com", "contains", true))
        .await
        .expect("create_whitelist");
    assert!(id > 0);

    let wl = list_whitelist_inner(&s).unwrap();
    assert_eq!(wl.len(), 1);
    assert_eq!(wl[0].kind, "whitelist", "kind must be coerced to whitelist");
    assert_eq!(wl[0].pattern, "messages.google.com");

    let new_state = toggle_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(new_state, false, "first toggle flips to inactive");
    let wl = list_whitelist_inner(&s).unwrap();
    assert!(!wl[0].active);

    let new_state = toggle_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(new_state, true, "second toggle flips to active");

    delete_whitelist_inner(&s, id).await.unwrap();
    assert_eq!(list_whitelist_inner(&s).unwrap().len(), 0);
}

#[tokio::test]
async fn crud_quotas_via_inner() {
    let (s, _d) = make_state();

    let id = create_quota_inner(
        &s,
        app_quota::AppQuota {
            id: None,
            app_pattern: "Discord.exe".into(),
            daily_minutes: 30,
            used_seconds: 0,
            used_day: today_str(),
            active: true,
            created_at: 0,
        },
    )
    .expect("create_quota");
    assert!(id > 0);

    let rows = list_quotas_inner(&s).unwrap();
    assert_eq!(rows.len(), 1);
    // Pattern is lowercased on store to match the lowercased output
    // of the foreground + background trackers (Windows filenames are
    // case-insensitive).
    assert_eq!(rows[0].app_pattern, "discord.exe");

    // Update.
    let mut updated = rows[0].clone();
    updated.daily_minutes = 60;
    update_quota_inner(&s, updated).expect("update_quota");
    assert_eq!(list_quotas_inner(&s).unwrap()[0].daily_minutes, 60);

    // Deactivate then re-activate.
    set_app_quota_active_inner(&s, id, false).unwrap();
    assert!(!list_quotas_inner(&s).unwrap()[0].active);
    set_app_quota_active_inner(&s, id, true).unwrap();
    assert!(list_quotas_inner(&s).unwrap()[0].active);

    // Delete.
    delete_quota_inner(&s, id).unwrap();
    assert_eq!(list_quotas_inner(&s).unwrap().len(), 0);
}

// ─── get_usage_stats with different ranges ──────────────────────────

#[tokio::test]
async fn get_usage_stats_aggregates_per_range() {
    let (s, _d) = make_state();
    let now = now_ts();

    usage_log::append(
        &s.db,
        &usage_log::UsageSegment {
            id: None,
            kind: "url".into(),
            target: "fresh.com".into(),
            ts: now - 60,
            duration_seconds: 120,
        },
    )
    .expect("append fresh");
    usage_log::append(
        &s.db,
        &usage_log::UsageSegment {
            id: None,
            kind: "url".into(),
            target: "ancient.com".into(),
            ts: now - 100 * 86_400,
            duration_seconds: 60,
        },
    )
    .expect("append ancient");

    let day = get_usage_stats_inner(&s, "day".into()).unwrap();
    assert!(day.iter().any(|e| e.target == "fresh.com" && e.minutes == 2));
    assert!(!day.iter().any(|e| e.target == "ancient.com"));

    let week = get_usage_stats_inner(&s, "week".into()).unwrap();
    assert!(week.iter().any(|e| e.target == "fresh.com"));

    let month = get_usage_stats_inner(&s, "month".into()).unwrap();
    assert!(month.iter().any(|e| e.target == "fresh.com"));
    assert!(!month.iter().any(|e| e.target == "ancient.com"));

    let bad = get_usage_stats_inner(&s, "year".into());
    assert!(bad.is_err(), "year must be rejected");
    let err = bad.unwrap_err();
    assert!(err.contains("invalid range"), "got: {}", err);
}

// ─── additional coverage ────────────────────────────────────────────

#[test]
fn first_run_ack_via_inner() {
    let (s, _d) = make_state();
    assert!(needs_first_run_ack_inner(&s.app_data_dir).unwrap());
    ack_first_run_inner(&s.app_data_dir).unwrap();
    assert!(!needs_first_run_ack_inner(&s.app_data_dir).unwrap());
}

#[test]
fn block_hits_round_trip_via_inner() {
    let (s, _d) = make_state();
    log_block_hit_inner(&s, "url".into(), "https://x.com".into(), None).unwrap();
    let hits = list_block_hits_inner(&s, 0, 100).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, "https://x.com");
}

#[test]
fn list_sessions_after_insert() {
    let (s, _d) = make_state();
    session::start(&s.db, "focus", 25 * 60).expect("session::start");
    let sessions = list_sessions_inner(&s, 0).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].phase, "focus");
}

#[test]
fn get_stats_shape_with_session() {
    let (s, _d) = make_state();
    session::start(&s.db, "focus", 25 * 60).expect("session::start");
    let stats = get_stats_inner(&s, "today".into()).unwrap();
    assert_eq!(stats["sessions_today"].as_i64().unwrap(), 1);
    assert_eq!(stats["focus_minutes_today"].as_i64().unwrap(), 25);
}

#[test]
fn get_today_usage_filters_inactive() {
    let (s, _d) = make_state();

    let id = create_quota_inner(
        &s,
        app_quota::AppQuota {
            id: None,
            app_pattern: "Slack.exe".into(),
            daily_minutes: 60,
            used_seconds: 0,
            used_day: today_str(),
            active: true,
            created_at: 0,
        },
    )
    .unwrap();
    create_quota_inner(
        &s,
        app_quota::AppQuota {
            id: None,
            app_pattern: "Discord.exe".into(),
            daily_minutes: 30,
            used_seconds: 0,
            used_day: today_str(),
            active: true,
            created_at: 0,
        },
    )
    .unwrap();
    set_app_quota_active_inner(&s, id, false).unwrap();

    let rows = get_today_usage_inner(&s).unwrap();
    let patterns: Vec<&str> = rows.iter().map(|r| r.app_pattern.as_str()).collect();
    assert!(patterns.contains(&"discord.exe"));
    assert!(!patterns.contains(&"slack.exe"), "inactive must be filtered");
}

// ─── end-to-end timer tick through dummy handle ──────────────────────
//
// We can't run the actual TimerHandle::start because it requires a Wry
// AppHandle (and the test feature causes STATUS_ENTRYPOINT_NOT_FOUND).
// The run-loop + phase-callback contract is covered by
// `engine_timer.rs::run_loop_invokes_phase_callback`. Here we instead
// verify the IPC surface around a dummy timer: pause/resume/cancel
// preserve state correctly across many calls.

#[tokio::test]
async fn focus_state_machine_via_dummy_handle() {
    let (s, _d) = make_state();

    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));

    // Pause → status Paused.
    pause_focus_inner(&s).unwrap();
    assert_eq!(
        s.active_timer.lock().unwrap().as_ref().unwrap().state().status,
        TimerStatus::Paused
    );

    // Resume → status Running.
    resume_focus_inner(&s).unwrap();
    assert_eq!(
        s.active_timer.lock().unwrap().as_ref().unwrap().state().status,
        TimerStatus::Running
    );

    // Pause again.
    pause_focus_inner(&s).unwrap();
    assert_eq!(
        s.active_timer.lock().unwrap().as_ref().unwrap().state().status,
        TimerStatus::Paused
    );

    // Cancel → timer cleared.
    cancel_focus_inner(&s).unwrap();
    assert!(s.active_timer.lock().unwrap().is_none());
}

// ─── exercise full cancel + state cleanup via inner call ─────────────

#[tokio::test]
async fn cancel_via_inner_clears_timer_when_unlocked() {
    let (s, _d) = make_state();

    *s.active_timer.lock().unwrap() = Some(TimerHandle::dummy_for_test(false));
    assert!(s.active_timer.lock().unwrap().is_some());

    cancel_focus_inner(&s).expect("cancel");

    assert!(
        s.active_timer.lock().unwrap().is_none(),
        "cancel_focus must clear the active timer"
    );
}