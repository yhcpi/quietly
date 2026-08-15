use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tauri::{State, AppHandle};
use crate::storage::{
    block_rule::{self, BlockRule},
    schedule::{self, Schedule},
    session::{self, Session},
    block_hit::{self, BlockHit},
    settings,
    db::DbPool,
};
use crate::engine::timer::{TimerHandle, TimerConfig};
use crate::engine::proxy::{self, ProxyHandle};
use crate::engine::proxy_state::{self, ProxySnapshot};
use crate::engine::guard::{self, GuardHandle};

pub struct AppState {
    pub db: DbPool,
    pub rules: Arc<RwLock<Vec<BlockRule>>>,
    pub active_timer: Mutex<Option<TimerHandle>>,
    pub active_proxy: Mutex<Option<ProxyHandle>>,
    pub active_guard: Mutex<Option<GuardHandle>>,
    pub proxy_snapshot: Mutex<Option<ProxySnapshot>>,
}

impl AppState {
    pub fn new(db: DbPool) -> Self {
        // Load initial rules
        let initial_rules = block_rule::list(&db).unwrap_or_default();
        Self {
            db,
            rules: Arc::new(RwLock::new(initial_rules)),
            active_timer: Mutex::new(None),
            active_proxy: Mutex::new(None),
            active_guard: Mutex::new(None),
            proxy_snapshot: Mutex::new(None),
        }
    }

    /// Refresh the shared rules cache from the DB.
    pub async fn refresh_rules(&self) {
        if let Ok(rules) = block_rule::list(&self.db) {
            *self.rules.write().await = rules;
        }
    }
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ─── block rules ──────────────────────────────────────────────────────

pub fn list_block_rules_inner(state: &AppState) -> Result<Vec<BlockRule>, String> {
    block_rule::list(&state.db).map_err(err)
}

pub fn create_block_rule_inner(state: &AppState, mut rule: BlockRule) -> Result<i64, String> {
    if rule.kind != "url" && rule.kind != "domain" && rule.kind != "app_path" {
        return Err(format!("invalid kind: {}", rule.kind));
    }
    if rule.match_type != "exact" && rule.match_type != "contains" && rule.match_type != "regex" {
        return Err(format!("invalid match_type: {}", rule.match_type));
    }
    rule.id = None;
    let id = block_rule::create(&state.db, &rule).map_err(err)?;
    Ok(id)
}

pub fn update_block_rule_inner(state: &AppState, rule: BlockRule) -> Result<(), String> {
    block_rule::update(&state.db, &rule).map_err(err)
}

pub fn delete_block_rule_inner(state: &AppState, id: i64) -> Result<(), String> {
    block_rule::delete(&state.db, id).map_err(err)
}

pub fn set_block_rule_active_inner(state: &AppState, id: i64, active: bool) -> Result<(), String> {
    block_rule::set_active(&state.db, id, active).map_err(err)
}

// ─── schedules ────────────────────────────────────────────────────────

pub fn list_schedules_inner(state: &AppState) -> Result<Vec<Schedule>, String> {
    schedule::list(&state.db).map_err(err)
}

pub fn create_schedule_inner(state: &AppState, mut s: Schedule) -> Result<i64, String> {
    if s.kind != "timer" && s.kind != "weekly" {
        return Err(format!("invalid schedule kind: {}", s.kind));
    }
    s.id = None;
    schedule::create(&state.db, &s).map_err(err)
}

pub fn update_schedule_inner(state: &AppState, s: Schedule) -> Result<(), String> {
    schedule::update(&state.db, &s).map_err(err)
}

pub fn delete_schedule_inner(state: &AppState, id: i64) -> Result<(), String> {
    schedule::delete(&state.db, id).map_err(err)
}

pub fn toggle_schedule_inner(state: &AppState, id: i64, enabled: bool) -> Result<(), String> {
    schedule::toggle(&state.db, id, enabled).map_err(err)
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

pub fn log_block_hit_inner(state: &AppState, kind: String, target: String, rule_id: Option<i64>, schedule_id: Option<i64>) -> Result<(), String> {
    block_hit::log(&state.db, &kind, &target, rule_id, schedule_id).map_err(err)
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

pub fn start_focus_inner(state: &AppState, app: AppHandle, duration_min: u32) -> Result<i64, String> {
    let mut guard = state.active_timer.lock().unwrap();
    if guard.is_some() {
        return Err("focus session already active".into());
    }
    let session_id = session::start(&state.db, None, "focus", duration_min as i64 * 60).map_err(err)?;
    let cfg = TimerConfig { focus_min: duration_min, short_break_min: 5, long_break_min: 15, cycles_before_long: 4 };
    let handle = TimerHandle::start(app, cfg);
    *guard = Some(handle);
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

pub fn cancel_focus_inner(state: &AppState) -> Result<(), String> {
    let mut guard = state.active_timer.lock().unwrap();
    let handle = guard.take().ok_or_else(|| "no active timer".to_string())?;
    handle.cancel();
    Ok(())
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

    // Snapshot current state and set system proxy
    let snap = proxy_state::read_current().map_err(err)?;
    proxy_state::set_proxy(&format!("127.0.0.1:{}", port)).map_err(err)?;
    *state.proxy_snapshot.lock().unwrap() = Some(snap);

    // Start local proxy
    let handle = proxy::start(app, port, state.rules.clone()).await.map_err(err)?;

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
    if let Some(snap) = state.proxy_snapshot.lock().unwrap().take() {
        let _ = proxy_state::restore(&snap);
    }
    Ok(())
}

// ─── guard (state) ──────────────────────────────────────────────────

pub fn start_guard_inner(state: &AppState, app: AppHandle) -> Result<(), String> {
    let mut guard = state.active_guard.lock().unwrap();
    if guard.is_some() {
        return Err("guard already running".into());
    }
    let handle = guard::start(app, state.rules.clone()).map_err(err)?;
    *guard = Some(handle);
    Ok(())
}

pub fn stop_guard_inner(state: &AppState) -> Result<(), String> {
    let mut guard = state.active_guard.lock().unwrap();
    let handle = guard.take().ok_or_else(|| "guard not running".to_string())?;
    handle.stop().map_err(err)
}

// ─── Tauri command wrappers ──────────────────────────────────────────

#[tauri::command]
pub fn list_block_rules(state: State<AppState>) -> Result<Vec<BlockRule>, String> {
    list_block_rules_inner(&state)
}

#[tauri::command]
pub fn create_block_rule(state: State<AppState>, rule: BlockRule) -> Result<i64, String> {
    create_block_rule_inner(&state, rule)
}

#[tauri::command]
pub fn update_block_rule(state: State<AppState>, rule: BlockRule) -> Result<(), String> {
    update_block_rule_inner(&state, rule)
}

#[tauri::command]
pub fn delete_block_rule(state: State<AppState>, id: i64) -> Result<(), String> {
    delete_block_rule_inner(&state, id)
}

#[tauri::command]
pub fn set_block_rule_active(state: State<AppState>, id: i64, active: bool) -> Result<(), String> {
    set_block_rule_active_inner(&state, id, active)
}

#[tauri::command]
pub fn list_schedules(state: State<AppState>) -> Result<Vec<Schedule>, String> {
    list_schedules_inner(&state)
}

#[tauri::command]
pub fn create_schedule(state: State<AppState>, schedule: Schedule) -> Result<i64, String> {
    create_schedule_inner(&state, schedule)
}

#[tauri::command]
pub fn update_schedule(state: State<AppState>, schedule: Schedule) -> Result<(), String> {
    update_schedule_inner(&state, schedule)
}

#[tauri::command]
pub fn delete_schedule(state: State<AppState>, id: i64) -> Result<(), String> {
    delete_schedule_inner(&state, id)
}

#[tauri::command]
pub fn toggle_schedule(state: State<AppState>, id: i64, enabled: bool) -> Result<(), String> {
    toggle_schedule_inner(&state, id, enabled)
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
pub fn log_block_hit(state: State<AppState>, kind: String, target: String, rule_id: Option<i64>, schedule_id: Option<i64>) -> Result<(), String> {
    log_block_hit_inner(&state, kind, target, rule_id, schedule_id)
}

#[tauri::command]
pub fn list_sessions(state: State<AppState>, since_ts: i64) -> Result<Vec<Session>, String> {
    list_sessions_inner(&state, since_ts)
}

#[tauri::command]
pub fn start_focus(app: AppHandle, state: State<AppState>, duration_min: u32) -> Result<i64, String> {
    start_focus_inner(&state, app, duration_min)
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
pub fn cancel_focus(state: State<AppState>) -> Result<(), String> {
    cancel_focus_inner(&state)
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