use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tauri::{Manager, State, AppHandle};

use crate::storage::{
    self,
    block_rule::{self, BlockRule},
    session::{self, Session},
    block_hit::{self, BlockHit},
    settings,
    db::DbPool,
    app_quota::{self, AppQuota},
    usage_log,
};
use crate::engine::timer::{TimerHandle, TimerConfig, Phase};
use crate::engine::pac;
use crate::engine::proxy::{self, ProxyHandle};
use crate::engine::proxy_state::{self, ProxySnapshot};
use crate::engine::guard::{self, GuardHandle};
use crate::engine::app_usage::{self, UsageHandle};

pub struct AppState {
    pub db: DbPool,
    pub rules: Arc<RwLock<Vec<BlockRule>>>,
    pub active_timer: Mutex<Option<TimerHandle>>,
    pub active_proxy: Mutex<Option<ProxyHandle>>,
    pub active_guard: Mutex<Option<GuardHandle>>,
    pub active_app_usage: Mutex<Option<UsageHandle>>,
    pub proxy_snapshot: Mutex<Option<ProxySnapshot>>,
    pub app_data_dir: std::path::PathBuf,
}

impl AppState {
    pub fn new(db: DbPool, app_data_dir: std::path::PathBuf) -> Self {
        // Load initial rules
        let initial_rules = block_rule::list(&db).unwrap_or_default();
        Self {
            db,
            rules: Arc::new(RwLock::new(initial_rules)),
            active_timer: Mutex::new(None),
            active_proxy: Mutex::new(None),
            active_guard: Mutex::new(None),
            active_app_usage: Mutex::new(None),
            proxy_snapshot: Mutex::new(None),
            app_data_dir,
        }
    }

    /// Refresh the shared rules cache from the DB.
    pub async fn refresh_rules(&self) {
        if let Ok(rules) = block_rule::list(&self.db) {
            *self.rules.write().await = rules;
        }
    }

    /// Start the engine-side usage tracker. Idempotent: if one is
    /// already running, returns Ok without restarting. We keep a
    /// single tracker alive across both proxy and guard lifecycles
    /// so URL / app / title updates from either source flush in the
    /// same 10s window.
    pub fn start_app_usage(&self, app: AppHandle) -> Result<(), String> {
        let mut g = self.active_app_usage.lock().unwrap();
        if g.is_some() { return Ok(()); }
        *g = Some(app_usage::start(app, self.db.clone()));
        Ok(())
    }

    /// Stop the engine-side usage tracker. No-op if not running.
    pub fn stop_app_usage(&self) {
        let h = self.active_app_usage.lock().unwrap().take();
        if let Some(h) = h { h.stop(); }
    }
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ─── block rules ──────────────────────────────────────────────────────

pub fn list_block_rules_inner(state: &AppState) -> Result<Vec<BlockRule>, String> {
    block_rule::list(&state.db).map_err(err)
}

pub async fn create_block_rule_inner(state: &AppState, mut rule: BlockRule) -> Result<i64, String> {
    if !matches!(rule.kind.as_str(), "url" | "domain" | "app_path" | "title" | "whitelist") {
        return Err(format!("invalid kind: {}", rule.kind));
    }
    if rule.match_type != "exact" && rule.match_type != "contains" && rule.match_type != "regex" {
        return Err(format!("invalid match_type: {}", rule.match_type));
    }
    rule.id = None;
    let id = block_rule::create(&state.db, &rule).map_err(err)?;
    state.refresh_rules().await;
    Ok(id)
}

pub async fn update_block_rule_inner(state: &AppState, rule: BlockRule) -> Result<(), String> {
    block_rule::update(&state.db, &rule).map_err(err)?;
    state.refresh_rules().await;
    Ok(())
}

pub async fn delete_block_rule_inner(state: &AppState, id: i64) -> Result<(), String> {
    block_rule::delete(&state.db, id).map_err(err)?;
    state.refresh_rules().await;
    Ok(())
}

pub async fn set_block_rule_active_inner(state: &AppState, id: i64, active: bool) -> Result<(), String> {
    block_rule::set_active(&state.db, id, active).map_err(err)?;
    state.refresh_rules().await;
    Ok(())
}

// ─── settings ─────────────────────────────────────────────────────────

pub fn get_settings_inner(state: &AppState) -> Result<serde_json::Value, String> {
    let mut map = serde_json::Map::new();
    let stored = settings::get(&state.db, "json").unwrap_or_default();
    if let Some(s) = stored {
        let parsed: serde_json::Value = serde_json::from_str(&s).map_err(err)?;
        if let serde_json::Value::Object(obj) = parsed {
            map.extend(obj);
        }
    }
    Ok(serde_json::Value::Object(map))
}

pub fn set_settings_inner(state: &AppState, value: serde_json::Value) -> Result<(), String> {
    let s = serde_json::to_string(&value).map_err(err)?;
    settings::set(&state.db, "json", &s).map_err(err)
}

// ─── first-run ack ────────────────────────────────────────────────────

pub fn needs_first_run_ack_inner(state: &AppState) -> Result<bool, String> {
    Ok(settings::get(&state.db, "first_run_acked").map_err(err)?.is_none())
}

pub fn ack_first_run_inner(state: &AppState) -> Result<(), String> {
    settings::set(&state.db, "first_run_acked", "1").map_err(err)
}

// ─── block hits ───────────────────────────────────────────────────────

pub fn list_block_hits_inner(state: &AppState, since_ts: i64, limit: i64) -> Result<Vec<BlockHit>, String> {
    block_hit::list_recent(&state.db, since_ts, limit).map_err(err)
}

pub fn log_block_hit_inner(state: &AppState, kind: String, target: String, rule_id: Option<i64>) -> Result<(), String> {
    block_hit::log(&state.db, &kind, &target, rule_id).map_err(err)
}

// ─── sessions ─────────────────────────────────────────────────────────

pub fn list_sessions_inner(state: &AppState, since_ts: i64) -> Result<Vec<Session>, String> {
    session::list_recent(&state.db, since_ts).map_err(err)
}

// ─── stats ────────────────────────────────────────────────────────────

pub fn get_stats_inner(state: &AppState, _range: String) -> Result<serde_json::Value, String> {
    let since = now_ts() - 86400;  // last 24h as a simple default
    let sessions = session::list_recent(&state.db, since).map_err(err)?;
    let hits = block_hit::list_recent(&state.db, since, 10_000).map_err(err)?;
    let total_focus_seconds: i64 = sessions.iter().map(|s| s.duration_seconds).sum();
    let completed_sessions = sessions.iter().filter(|s| s.completed).count() as i64;
    Ok(serde_json::json!({
        "sessions_today": sessions.len() as i64,
        "completed_sessions": completed_sessions,
        "focus_minutes_today": total_focus_seconds / 60,
        "blocks_today": hits.len() as i64,
    }))
}

fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

// ─── focus timer (state) ─────────────────────────────────────────────

/// What the auto-block shield should do when the timer enters a new phase.
/// Pure value type so the decision logic can be unit-tested without Tauri.
#[derive(Debug, PartialEq, Eq)]
pub enum ShieldAction { Up, Down, NoOp }

/// Decide what the auto-block shield should do when the timer enters a
/// new phase. Focus wants the shield up; any break (Short or Long) wants
/// it down. The current state is taken from `state.active_proxy` so the
/// caller doesn't have to know whether the shield is currently up.
pub fn decide_shield_action(state: &AppState, new_phase: Phase) -> ShieldAction {
    let want_up = new_phase == Phase::Focus;
    let currently_up = state.active_proxy.lock().unwrap().is_some();
    match (want_up, currently_up) {
        (true, false) => ShieldAction::Up,
        (false, true) => ShieldAction::Down,
        _ => ShieldAction::NoOp,
    }
}

/// Wires phase-scoped auto-block: on entering Focus, the shield comes up;
/// on entering ShortBreak or LongBreak, the shield goes down. The shield
/// stays down until the next Focus phase. The callback fires on the
/// synchronous `on_phase_change` from the engine's run_loop, so we
/// `tauri::async_runtime::spawn` the actual start/stop work — the sync
/// callback returns immediately and doesn't block the timer.
///
/// Errors during start/stop are swallowed (best-effort): a missed
/// transition self-corrects on the next one.
pub struct AutoBlockController {
    app: AppHandle,
}

impl AutoBlockController {
    pub fn new(app: AppHandle) -> Self { Self { app } }

    /// Consume self, return a `'static + Send` closure suitable for
    /// `TimerHandle::start`'s `on_phase_change_extra` parameter.
    pub fn into_callback(self) -> Box<dyn Fn(Phase, Phase) + Send + 'static> {
        Box::new(move |_old: Phase, new: Phase| {
            let app = self.app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                match decide_shield_action(&state, new) {
                    ShieldAction::Up => {
                        let _ = start_proxy_inner(
                            &state, app.clone(), TimerHandle::AUTO_BLOCK_PROXY_PORT,
                        ).await;
                        let _ = start_guard_inner(&state, app.clone());
                    }
                    ShieldAction::Down => {
                        let _ = stop_guard_inner(&state);
                        let _ = stop_proxy_inner(&state).await;
                    }
                    ShieldAction::NoOp => {}
                }
            });
        })
    }
}

pub async fn start_focus_inner(
    state: &AppState,
    app: AppHandle,
    duration_min: u32,
    locked: bool,
    auto_block: bool,
) -> Result<i64, String> {
    // Build the optional phase callback BEFORE acquiring the timer mutex
    // so the controller's AppHandle clone doesn't extend the borrow
    // across the inner await below. The callback is owned by the spawned
    // timer task for the duration of the session.
    let phase_cb = if auto_block {
        Some(AutoBlockController::new(app.clone()).into_callback())
    } else {
        None
    };

    // Scope the mutex acquisition so the guard is released before any
    // .await below — holding a std::sync::MutexGuard across an await is
    // both a footgun (Send-ness) and a deadlock risk for the inner
    // start_proxy_inner which itself takes the active_proxy mutex.
    let session_id = {
        let mut guard = state.active_timer.lock().unwrap();
        if guard.is_some() {
            return Err("focus session already active".into());
        }
        let id = session::start(&state.db, "focus", duration_min as i64 * 60).map_err(err)?;
        let cfg = TimerConfig { focus_min: duration_min, short_break_min: 5, long_break_min: 15, cycles_before_long: 4 };
        let handle = TimerHandle::start(app.clone(), cfg, locked, phase_cb);
        *guard = Some(handle);
        id
    };

    // Auto-block: bring proxy + guard up synchronously so the shield is
    // live by the first tick (covers the initial Focus phase, which the
    // phase callback never sees — it only fires on transitions). Errors
    // here fail the whole start — a focus session without a shield is
    // worse than no session. The phase callback takes over for every
    // subsequent transition (Focus↔break).
    if auto_block {
        start_proxy_inner(state, app.clone(), TimerHandle::AUTO_BLOCK_PROXY_PORT).await.map_err(err)?;
        start_guard_inner(state, app).map_err(err)?;
    }

    Ok(session_id)
}

pub fn pause_focus_inner(state: &AppState) -> Result<(), String> {
    let guard = state.active_timer.lock().unwrap();
    let handle = guard.as_ref().ok_or_else(|| "no active timer".to_string())?;
    handle.pause();
    Ok(())
}

pub fn resume_focus_inner(state: &AppState) -> Result<(), String> {
    let guard = state.active_timer.lock().unwrap();
    let handle = guard.as_ref().ok_or_else(|| "no active timer".to_string())?;
    handle.resume();
    Ok(())
}

/// Cancel the active focus session. If the session was started with
/// `locked=true`, the handle is put back and an error is returned so the
/// session keeps running. On a non-locked cancel the timer handle is
/// cancelled synchronously; the Tauri command wrapper performs the async
/// guard/proxy teardown via `cancel_focus_teardown`.
pub fn cancel_focus_inner(state: &AppState) -> Result<(), String> {

    let mut guard = state.active_timer.lock().unwrap();
    let h = match guard.take() {
        Some(h) => h,
        None => {

            return Err("no active timer".to_string());
        }
    };
    if h.is_locked() {

        *guard = Some(h);
        return Err("session is locked — cancel disabled".into());
    }
    drop(guard); // release mutex before returning; handle is consumed by cancel()

    h.cancel();

    Ok(())
}

/// Async best-effort teardown of guard + proxy. Called by the Tauri
/// command wrapper after a successful (non-locked) cancel_focus_inner.
pub async fn cancel_focus_teardown(state: &AppState) {
    let _ = stop_guard_inner(state);
    let _ = stop_proxy_inner(state).await;
}

/// Toggle the `locked` flag on the active timer. Returns Err if no
/// session is active. MVP never invokes this from the UI but the IPC
/// surface is wired up for future use.
pub fn set_session_locked_inner(state: &AppState, locked: bool) -> Result<(), String> {
    let guard = state.active_timer.lock().unwrap();
    let handle = guard.as_ref().ok_or_else(|| "no active timer".to_string())?;
    handle.set_locked(locked);
    Ok(())
}

/// Returns the locked state of the active timer, or `false` if no timer
/// is running (the UI treats no-session and unlocked identically).
pub fn is_session_locked_inner(state: &AppState) -> Result<bool, String> {
    let guard = state.active_timer.lock().unwrap();
    Ok(guard.as_ref().map(|h| h.is_locked()).unwrap_or(false))
}

// ─── proxy (state, async) ────────────────────────────────────────────

pub async fn start_proxy_inner(state: &AppState, app: AppHandle, port: u16) -> Result<(), String> {
    // Check no proxy running (scope the lock so it doesn't cross .await).
    {
        let guard = state.active_proxy.lock().unwrap();
        if guard.is_some() {
            return Err("proxy already running".into());
        }
    }

    let app_data = app.path().app_data_dir().map_err(err)?;

    // Capture the user's pre-existing HKCU ProxyEnable / ProxyServer
    // state so we can restore it on stop_proxy (and on crash recovery
    // via the marker file).
    let snap = proxy_state::read_current().map_err(err)?;

    // Render + write the PAC file FIRST so that any tool that does
    // pick up AutoConfigURL (HTTP clients, scripts, the user's
    // VPN, future Edge versions) finds a valid file. The actual
    // active mechanism is the legacy HKCU ProxyServer below — the
    // PAC file is belt-and-suspenders and a safe default to leave
    // around if we crash before clearing AutoConfigURL.
    //
    // Must use `.read().await` (not `blocking_read`): we're inside
    // an async fn running on the tokio runtime worker, and
    // blocking_read panics with "Cannot block the current thread
    // from within a runtime" — which silently kills the worker and
    // makes the IPC call hang forever. (This was the root cause of
    // "start_focus hangs and the proxy never comes up".)
    let rules = state.rules.read().await.clone();
    let _pac_path = pac::write(&app_data, &rules).map_err(err)?;

    // Active mechanism: HKCU ProxyEnable=1 + ProxyServer=127.0.0.1:<port>.
    //
    // Why legacy ProxyServer instead of AutoConfigURL: Chromium-based
    // Edge/Chrome reads ProxyServer directly and routes through it,
    // but reliably ignores the per-connection PAC blob on some Windows
    // builds (root cause: WinINET vs WinHTTP query path / stale
    // per-connection cache). Setting ProxyServer to our port forces
    // every browser to forward CONNECT through our proxy; our proxy
    // returns 403 for blocked hosts. For HTTPS sites the browser
    // shows "ERR_TUNNEL_CONNECTION_FAILED" instead of the proxy's
    // 403 body — that's a Chrome protocol-level limitation, not
    // something we can fix without TLS interception.
    //
    // Crash-safety: if Quietly dies with the system pointing at a
    // dead port, all browsing hangs. Mitigated by:
    //   1. `stop_proxy_inner` calls `restore(&snap)` to put the
    //      user's original ProxyEnable / ProxyServer values back.
    //   2. `lib.rs` setup runs `proxy_marker::reap` to handle the
    //      crash case (stale marker → restore from snapshot).
    //   3. The tray Quit menu item invokes `graceful_shutdown` which
    //      calls stop_proxy — so a clean exit always restores.
    let proxy_spec = format!("127.0.0.1:{}", port);
    proxy_state::set_proxy(&proxy_spec).map_err(err)?;

    // WinINET cache invalidation: writing to HKCU is not enough —
    // running browsers cache proxy state and will ignore the new
    // value until their cache TTL expires. The
    // INTERNET_OPTION_SETTINGS_CHANGED broadcast forces every
    // WinINET client to re-read the registry on its next request.
    // Must run on the blocking thread pool: the Win32 call is a
    // synchronous broadcast that can take hundreds of ms while it
    // walks every top-level process. Calling it inline on the async
    // worker would block the runtime.
    let _ = tokio::task::spawn_blocking(proxy_state::notify_proxy_changed).await;

    // Stash the snapshot so stop_proxy_inner can restore it.
    *state.proxy_snapshot.lock().unwrap() = Some(snap.clone());

    // Persist a crash-recovery marker so the next launch can detect
    // "the previous session died mid-flight" and restore from it.
    let _ = storage::proxy_marker::write(
        &app_data,
        &storage::proxy_marker::ProxyMarker { port, snapshot: snap },
    );

    // Start local proxy
    let handle = proxy::start(app.clone(), port, state.rules.clone()).await.map_err(err)?;

    // App-usage tracker is started once on app setup (lib.rs) and runs
    // for the lifetime of the process — independent of any focus
    // session. Tracking must work even when no focus session is active,
    // otherwise a quota set up in the UI never accumulates time.

    // Store handle. Re-check inside lock for the small race window above.
    let mut guard = state.active_proxy.lock().unwrap();
    if guard.is_some() {
        return Err("proxy already running".into());
    }
    *guard = Some(handle);
    Ok(())
}

pub async fn stop_proxy_inner(state: &AppState) -> Result<(), String> {
    let handle_opt = {
        let mut guard = state.active_proxy.lock().unwrap();
        guard.take()
    };
    let handle = handle_opt.ok_or_else(|| "proxy not running".to_string())?;
    handle.stop().await;

    // Restore the user's original HKCU ProxyEnable / ProxyServer.
    // This is the inverse of `set_proxy(&proxy_spec)` in start_proxy_inner.
    // Best-effort — if the snapshot wasn't captured (very old crash),
    // we at least turn the proxy off so the user isn't stranded
    // pointing at a dead port.
    let snap = state.proxy_snapshot.lock().unwrap().take();
    match snap {
        Some(s) => {
            let _ = proxy_state::restore(&s);
        }
        None => {
            // Defensive fallback: no snapshot means we're in an
            // unknown prior state. The safest move is to disable the
            // proxy entirely; if the user had something else set up
            // they can re-enable it manually.
            let _ = proxy_state::clear_proxy();
        }
    }

    // CRASH-SAFETY INVARIANT: rewrite the PAC file to the "DIRECT
    // everything" safe default. The PAC file lives next to the db
    // and might still be referenced by AutoConfigURL on a separate
    // connection profile; keeping it as a valid DIRECT-only JS
    // function means a stray AutoConfigURL reader won't hang.
    let _ = pac::write_safe_default(&state.app_data_dir);
    let _ = proxy_state::clear_pac_url();
    // Per-connection PAC blob: clear it too so a stale reference in
    // the binary blob doesn't outlive our process.
    let _ = tokio::task::spawn_blocking(proxy_state::clear_pac_url_per_connection).await;
    // Same WinINET cache invalidation as start_proxy_inner — running
    // browsers must drop their cached proxy reference so subsequent
    // requests don't keep hitting our (now-dead) 47820 listener.
    // On the blocking pool so the synchronous broadcast doesn't
    // stall the async runtime.
    let _ = tokio::task::spawn_blocking(proxy_state::notify_proxy_changed).await;

    // Drop the crash-recovery marker — clean shutdown.
    storage::proxy_marker::clear(&state.app_data_dir);

    // App-usage tracker is NOT stopped here — it runs for the lifetime
    // of the process (see lib.rs setup). Stopping it when the proxy
    // goes down would leave quotas frozen whenever no focus session
    // is active.
    Ok(())
}

// ─── guard (state) ──────────────────────────────────────────────────

pub fn start_guard_inner(state: &AppState, app: AppHandle) -> Result<(), String> {
    let mut guard = state.active_guard.lock().unwrap();
    if guard.is_some() {
        return Err("guard already running".into());
    }
    let handle = guard::start(app.clone(), state.rules.clone()).map_err(err)?;
    // App-usage tracker runs for the lifetime of the process — see
    // lib.rs setup. Do not start/stop it here.
    *guard = Some(handle);
    Ok(())
}

pub fn stop_guard_inner(state: &AppState) -> Result<(), String> {
    let mut guard = state.active_guard.lock().unwrap();
    let handle = guard.take().ok_or_else(|| "guard not running".to_string())?;
    handle.stop().map_err(err)?;
    Ok(())
}

// ─── graceful shutdown ──────────────────────────────────────────────
//
// Best-effort teardown called from window-close and tray Quit. We deliberately
// ignore individual step failures — if proxy already stopped, that's fine —
// but we DO want every step to attempt, so a half-initialized session can't
// leak a stuck proxy. Order matches handleCancel in the UI: timer first
// (releases session state), guard, proxy (also restores the system proxy).

pub async fn graceful_shutdown(state: &AppState) {
    let _ = cancel_focus_inner(state);
    // Locked session: cancel was rejected — leave the shield up so the
    // session keeps its commitment to the user.
    if !is_session_locked_inner(state).unwrap_or(false) {
        let _ = stop_guard_inner(state);
        let _ = stop_proxy_inner(state).await;
    }
    // Stop the app-usage tracker here and ONLY here — it runs for the
    // lifetime of the process (started in lib.rs setup). Closing the
    // window no longer exits the app (it hides to tray), so the
    // tracker must survive that path. Only the tray Quit menu item
    // reaches this code, so usage tracking follows the real app
    // lifecycle rather than the focus-session lifecycle.
    state.stop_app_usage();
}

// ─── Tauri command wrappers ──────────────────────────────────────────

#[tauri::command]
pub async fn create_block_rule(state: State<'_, AppState>, rule: BlockRule) -> Result<i64, String> {
    create_block_rule_inner(&state, rule).await
}

#[tauri::command]
pub async fn update_block_rule(state: State<'_, AppState>, rule: BlockRule) -> Result<(), String> {
    update_block_rule_inner(&state, rule).await
}

#[tauri::command]
pub async fn delete_block_rule(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    delete_block_rule_inner(&state, id).await
}

#[tauri::command]
pub async fn set_block_rule_active(state: State<'_, AppState>, id: i64, active: bool) -> Result<(), String> {
    set_block_rule_active_inner(&state, id, active).await
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<serde_json::Value, String> {
    get_settings_inner(&state)
}

#[tauri::command]
pub fn set_settings(state: State<AppState>, settings: serde_json::Value) -> Result<(), String> {
    set_settings_inner(&state, settings)
}

#[tauri::command]
pub fn get_stats(state: State<AppState>, range: String) -> Result<serde_json::Value, String> {
    get_stats_inner(&state, range)
}

#[tauri::command]
pub fn needs_first_run_ack(state: State<AppState>) -> Result<bool, String> {
    needs_first_run_ack_inner(&state)
}

#[tauri::command]
pub fn ack_first_run(state: State<AppState>) -> Result<(), String> {
    ack_first_run_inner(&state)
}

#[tauri::command]
pub fn list_block_hits(state: State<AppState>, since_ts: i64, limit: i64) -> Result<Vec<BlockHit>, String> {
    list_block_hits_inner(&state, since_ts, limit)
}

#[tauri::command]
pub fn log_block_hit(state: State<AppState>, kind: String, target: String, rule_id: Option<i64>) -> Result<(), String> {
    log_block_hit_inner(&state, kind, target, rule_id)
}

#[tauri::command]
pub fn list_sessions(state: State<AppState>, since_ts: i64) -> Result<Vec<Session>, String> {
    list_sessions_inner(&state, since_ts)
}

#[tauri::command]
pub fn pause_focus(state: State<AppState>) -> Result<(), String> {
    pause_focus_inner(&state)
}

#[tauri::command]
pub fn resume_focus(state: State<AppState>) -> Result<(), String> {
    resume_focus_inner(&state)
}

#[tauri::command]
pub async fn start_proxy(app: AppHandle, state: State<'_, AppState>, port: u16) -> Result<(), String> {
    start_proxy_inner(&state, app, port).await
}

#[tauri::command]
pub async fn stop_proxy(state: State<'_, AppState>) -> Result<(), String> {
    stop_proxy_inner(&state).await
}

#[tauri::command]
pub fn start_guard(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    start_guard_inner(&state, app)
}

#[tauri::command]
pub fn stop_guard(state: State<AppState>) -> Result<(), String> {
    stop_guard_inner(&state)
}

// ─── whitelist (kind=whitelist convenience wrappers over block_rule) ──
//
// Whitelist is implemented as a value of `block_rule.kind` per design.md.
// These wrappers exist so the frontend has dedicated endpoints that hide
// the `kind` plumbing and so the kind validation stays in one place.

pub fn list_whitelist_inner(state: &AppState) -> Result<Vec<BlockRule>, String> {
    block_rule::list(&state.db).map_err(err).map(|rs| {
        rs.into_iter().filter(|r| r.kind == "whitelist").collect()
    })
}

pub async fn create_whitelist_inner(
    state: &AppState,
    mut rule: BlockRule,
) -> Result<i64, String> {
    rule.kind = "whitelist".into();
    create_block_rule_inner(state, rule).await
}

pub async fn delete_whitelist_inner(state: &AppState, id: i64) -> Result<(), String> {
    delete_block_rule_inner(state, id).await
}

pub async fn toggle_whitelist_inner(state: &AppState, id: i64) -> Result<bool, String> {
    // Toggle: read current, flip, write. Cheap — block_rule is small and
    // toggles are infrequent. Caller gets the new active state.
    let conn = state.db.get().map_err(err)?;
    let current: i64 = conn.query_row(
        "SELECT active FROM block_rule WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    ).map_err(err)?;
    let next = current == 0;
    set_block_rule_active_inner(state, id, next).await?;
    Ok(next)
}

// ─── app quota (storage-backed CRUD) ────────────────────────────────

pub fn list_quotas_inner(state: &AppState) -> Result<Vec<AppQuota>, String> {
    app_quota::list(&state.db).map_err(err)
}

pub fn create_quota_inner(state: &AppState, mut quota: AppQuota) -> Result<i64, String> {
    if quota.app_pattern.trim().is_empty() {
        return Err("app_pattern must not be empty".into());
    }
    if quota.daily_minutes <= 0 {
        return Err("daily_minutes must be > 0".into());
    }
    // Windows filenames are case-insensitive; tracker output is
    // lowercased, so we lowercase the pattern on the way in. Without
    // this a user-typed "Notepad.exe" stays in DB as "Notepad.exe"
    // and never matches the segments the tracker writes as
    // "notepad.exe".
    quota.app_pattern = quota.app_pattern.trim().to_lowercase();
    // Reject duplicates — design says "first match wins", which is
    // confusing UX if two rows race on the same pattern.
    let existing = app_quota::list(&state.db).map_err(err)?;
    if existing.iter().any(|q| q.app_pattern == quota.app_pattern) {
        return Err(format!("quota for '{}' already exists", quota.app_pattern));
    }
    quota.id = None;
    if quota.used_day.is_empty() {
        quota.used_day = today_local();
    }
    if quota.created_at == 0 {
        quota.created_at = now_ts();
    }
    app_quota::create(&state.db, &quota).map_err(err)
}

pub fn update_quota_inner(state: &AppState, mut quota: AppQuota) -> Result<(), String> {
    if quota.app_pattern.trim().is_empty() {
        return Err("app_pattern must not be empty".into());
    }
    if quota.daily_minutes <= 0 {
        return Err("daily_minutes must be > 0".into());
    }
    quota.app_pattern = quota.app_pattern.trim().to_lowercase();
    app_quota::update(&state.db, &quota).map_err(err)
}

pub fn delete_quota_inner(state: &AppState, id: i64) -> Result<(), String> {
    app_quota::delete(&state.db, id).map_err(err)
}

pub fn set_app_quota_active_inner(state: &AppState, id: i64, active: bool) -> Result<(), String> {
    app_quota::set_active(&state.db, id, active).map_err(err)
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct QuotaStatusEntry {
    pub app_pattern: String,
    pub used_minutes: i64,
    pub daily_minutes: i64,
    pub exceeded: bool,
}

/// Convenience view for the UI: today's per-quota used minutes + exceeded
/// flag. `used_minutes` is rounded down (truncated) to whole minutes to
/// match the dashboard's whole-minute display.
pub fn get_quota_status_inner(state: &AppState) -> Result<Vec<QuotaStatusEntry>, String> {
    let today = today_local();
    let rows = app_quota::list(&state.db).map_err(err)?;
    let mut out = Vec::with_capacity(rows.len());
    for q in rows {
        // Honor daily reset semantics for the "what does the UI see" path.
        let used_seconds = if q.used_day == today { q.used_seconds } else { 0 };
        let threshold = q.daily_minutes.saturating_mul(60);
        out.push(QuotaStatusEntry {
            app_pattern: q.app_pattern,
            used_minutes: used_seconds / 60,
            daily_minutes: q.daily_minutes,
            exceeded: used_seconds >= threshold,
        });
    }
    Ok(out)
}

/// Today's per-quota usage — same shape as `QuotaStatusEntry` but only
/// for active rows. Mirrors the design's `get_quota_status` but the user
/// brief specified `get_today_usage`; both names point at the same data.
pub fn get_today_usage_inner(state: &AppState) -> Result<Vec<QuotaStatusEntry>, String> {
    let today = today_local();
    let rows = app_quota::list_active(&state.db).map_err(err)?;
    let mut out = Vec::with_capacity(rows.len());
    for q in rows {
        let used_seconds = if q.used_day == today { q.used_seconds } else { 0 };
        let threshold = q.daily_minutes.saturating_mul(60);
        out.push(QuotaStatusEntry {
            app_pattern: q.app_pattern,
            used_minutes: used_seconds / 60,
            daily_minutes: q.daily_minutes,
            exceeded: used_seconds >= threshold,
        });
    }
    Ok(out)
}

// ─── usage stats ─────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct UsageEntry {
    pub target: String,
    pub minutes: i64,
    pub kind: String,
}

/// Aggregate usage over a range. `range` is one of "day" (24h), "week"
/// (7d), "month" (30d). Returns both `app` and `url` entries combined —
/// the frontend separates them by `kind`.
pub fn get_usage_stats_inner(state: &AppState, range: String) -> Result<Vec<UsageEntry>, String> {
    let since = match range.as_str() {
        "day" => now_ts() - 86_400,
        "week" => now_ts() - 7 * 86_400,
        "month" => now_ts() - 30 * 86_400,
        _ => return Err(format!("invalid range: {}", range)),
    };
    let mut out = Vec::new();
    for kind in ["app", "url"] {
        for u in usage_log::aggregate_by_target(&state.db, kind, since).map_err(err)? {
            out.push(UsageEntry {
                target: u.target,
                minutes: u.total_seconds / 60,
                kind: kind.into(),
            });
        }
    }
    Ok(out)
}

// ─── local-time helpers ──────────────────────────────────────────────

fn today_local() -> String {
    use chrono::Local;
    Local::now().format("%Y-%m-%d").to_string()
}

// ─── Tauri command wrappers (new) ────────────────────────────────────

#[tauri::command]
pub fn list_block_rules(state: State<AppState>) -> Result<Vec<BlockRule>, String> {
    list_block_rules_inner(&state)
}

#[tauri::command]
pub fn list_whitelist(state: State<AppState>) -> Result<Vec<BlockRule>, String> {
    list_whitelist_inner(&state)
}

#[tauri::command]
pub async fn create_whitelist(state: State<'_, AppState>, rule: BlockRule) -> Result<i64, String> {
    create_whitelist_inner(&state, rule).await
}

#[tauri::command]
pub async fn delete_whitelist(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    delete_whitelist_inner(&state, id).await
}

#[tauri::command]
pub async fn toggle_whitelist(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    toggle_whitelist_inner(&state, id).await
}

#[tauri::command]
pub async fn start_focus(
    app: AppHandle,
    state: State<'_, AppState>,
    duration_min: u32,
    locked: bool,
    auto_block: bool,
) -> Result<i64, String> {
    start_focus_inner(&state, app, duration_min, locked, auto_block).await
}

#[tauri::command]
pub async fn cancel_focus(state: State<'_, AppState>) -> Result<(), String> {
    cancel_focus_inner(&state)?;
    cancel_focus_teardown(&state).await;
    Ok(())
}

#[tauri::command]
pub fn set_session_locked(state: State<AppState>, locked: bool) -> Result<(), String> {
    set_session_locked_inner(&state, locked)
}

#[tauri::command]
pub fn is_session_locked(state: State<AppState>) -> Result<bool, String> {
    is_session_locked_inner(&state)
}

#[tauri::command]
pub fn list_quotas(state: State<AppState>) -> Result<Vec<AppQuota>, String> {
    list_quotas_inner(&state)
}

#[tauri::command]
pub fn create_quota(state: State<AppState>, quota: AppQuota) -> Result<i64, String> {
    create_quota_inner(&state, quota)
}

#[tauri::command]
pub fn update_quota(state: State<AppState>, quota: AppQuota) -> Result<(), String> {
    update_quota_inner(&state, quota)
}

#[tauri::command]
pub fn delete_quota(state: State<AppState>, id: i64) -> Result<(), String> {
    delete_quota_inner(&state, id)
}

#[tauri::command]
pub fn set_app_quota_active(state: State<AppState>, id: i64, active: bool) -> Result<(), String> {
    set_app_quota_active_inner(&state, id, active)
}

#[tauri::command]
pub fn get_quota_status(state: State<AppState>) -> Result<Vec<QuotaStatusEntry>, String> {
    get_quota_status_inner(&state)
}

#[tauri::command]
pub fn get_today_usage(state: State<AppState>) -> Result<Vec<QuotaStatusEntry>, String> {
    get_today_usage_inner(&state)
}

#[tauri::command]
pub fn get_usage_stats(state: State<AppState>, range: String) -> Result<Vec<UsageEntry>, String> {
    get_usage_stats_inner(&state, range)
}