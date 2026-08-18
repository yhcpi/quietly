//! Engine-side usage tracking & quota accumulation.
//!
//! Background task that wakes every 10 s, snapshots the current
//! (app, url, title), and on change commits a usage segment to
//! `usage_log` and increments the matching active quota. The task
//! itself is tiny; the bulk of the logic lives in `flush_now`
//! which is exposed for tests so they can drive state transitions
//! deterministically without waiting for the timer.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast;
use tauri::async_runtime::JoinHandle;
use tokio::time::{interval, Duration};
use tauri::{AppHandle, Emitter};

use crate::storage::{
    app_quota::{self, AppQuota},
    db::DbPool,
    usage_log::{self, UsageSegment},
};

// Re-exports so sibling modules (background_tracker) don't need to
// depend on the storage crate's internal organization.
pub use app_quota::AppQuota as AppQuotaReexport;
pub use usage_log::UsageSegment as UsageSegmentReexport;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CurrentState {
    pub app: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug)]
struct Snapshot {
    state: CurrentState,
    last_change_ts: i64,
}

/// Shared state of the engine: latest observed (app, url, title) and
/// the timestamp the trio last changed. Held inside an `Arc<Mutex<…>>`
/// so the proxy (async) and guard (sync — Win32 callback) can write
/// to it from their threads and the background flush task can read
/// & clear the `changed` flag.
///
/// We use `std::sync::Mutex` (not tokio's) because the guard's
/// `extern "system"` callback cannot `.await`, and the lock is never
/// held across anything other than a few branch comparisons.
#[derive(Clone)]
pub struct UsageTracker {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    snap: Snapshot,
    changed: bool,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snap: Snapshot {
                    state: CurrentState::default(),
                    last_change_ts: now_ts(),
                },
                changed: false,
            })),
        }
    }

    /// Update fields and mark changed. Sync because the guard's Win32
    /// callback must be able to call it. Passing `None` for a field
    /// is a no-op for that field (we don't want a proxy-only URL
    /// update to wipe the app).
    ///
    /// `now` is the timestamp to stamp as the new change time. Live
    /// callers pass `now_ts()`; tests pass a synthetic clock.
    pub fn update_at(
        &self,
        app: Option<&str>,
        url: Option<&str>,
        title: Option<&str>,
        now: i64,
    ) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut touched = false;
        if let Some(a) = app {
            if g.snap.state.app.as_deref() != Some(a) {
                g.snap.state.app = Some(a.to_string());
                touched = true;
            }
        }
        if let Some(u) = url {
            if g.snap.state.url.as_deref() != Some(u) {
                g.snap.state.url = Some(u.to_string());
                touched = true;
            }
        }
        if let Some(t) = title {
            if g.snap.state.title.as_deref() != Some(t) {
                g.snap.state.title = Some(t.to_string());
                touched = true;
            }
        }
        if touched && !g.changed {
            g.snap.last_change_ts = now;
            g.changed = true;
        }
    }

    /// Convenience for live callers: stamp `now_ts()` onto the change.
    pub fn update(&self, app: Option<&str>, url: Option<&str>, title: Option<&str>) {
        self.update_at(app, url, title, now_ts());
    }

    /// Read-only view for tests.
    pub fn snapshot(&self) -> CurrentState {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.snap.state.clone()
    }

    /// Read state and last_change_ts together for the flush task.
    pub fn snapshot_for_flush(&self) -> (CurrentState, i64) {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        (g.snap.state.clone(), g.snap.last_change_ts)
    }

    /// Advance the "anchor" used by `flush_now` to compute the next
    /// segment's duration. Called by `flush_now` after writing, so
    /// subsequent flushes measure time from the previous flush (not
    /// from the most recent foreground change).
    pub fn advance_anchor(&self, now: i64) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.snap.last_change_ts = now;
        g.changed = false;
    }
}

/// Payload emitted on the `quota-exceeded` event. Kept small —
/// the frontend only needs the three fields to drive the toast.
#[derive(Serialize, Clone, Debug)]
pub struct QuotaExceededPayload {
    pub app_pattern: String,
    pub used_seconds: i64,
    pub daily_minutes: i64,
}

pub(crate) fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Storage handles the flush task needs. `Default` impls let tests
/// build a stub easily.
pub trait FlushStore: Send + Sync + 'static {
    fn append(&self, seg: &UsageSegment) -> anyhow::Result<i64>;
    fn increment_quota(
        &self,
        pattern: &str,
        delta_seconds: i64,
        today: &str,
    ) -> anyhow::Result<(i64, bool)>;
    fn lookup_active_quota(&self, pattern: &str) -> anyhow::Result<Option<AppQuota>>;
    fn list_active_quotas(&self) -> anyhow::Result<Vec<AppQuota>>;
}

/// Production store backed by the shared `DbPool`.
pub struct DbStore {
    pub pool: DbPool,
}

impl FlushStore for DbStore {
    fn append(&self, seg: &UsageSegment) -> anyhow::Result<i64> {
        usage_log::append(&self.pool, seg)
    }
    fn increment_quota(
        &self,
        pattern: &str,
        delta_seconds: i64,
        today: &str,
    ) -> anyhow::Result<(i64, bool)> {
        app_quota::increment_used(&self.pool, pattern, delta_seconds, today)
    }
    fn lookup_active_quota(&self, pattern: &str) -> anyhow::Result<Option<AppQuota>> {
        let rows = app_quota::list_active(&self.pool)?;
        Ok(rows.into_iter().find(|q| q.app_pattern == pattern))
    }
    fn list_active_quotas(&self) -> anyhow::Result<Vec<AppQuota>> {
        app_quota::list_active(&self.pool)
    }
}

/// Tauri emitter — trivial newtype so tests can swap a no-op.
pub trait EventSink: Send + Sync + 'static {
    fn emit_quota_exceeded(&self, payload: QuotaExceededPayload);
}

pub struct TauriSink {
    pub app: AppHandle,
}

impl EventSink for TauriSink {
    fn emit_quota_exceeded(&self, payload: QuotaExceededPayload) {
        // 1. Native OS toast so the user sees it even when their
        //    active window is a fullscreen game. Tauri's notification
        //    plugin surfaces as a Windows 10/11 toast, which the
        //    Action Center records even if Quietly is hidden.
        //    Clicking the toast focuses Quietly so the user can drill
        //    into the Quotas page.
        let used_minutes = payload.used_seconds / 60;
        let _ = self
            .app
            .notification()
            .builder()
            .title("Quota Exceeded")
            .body(format!(
                "{} — {} of {} min today. Time to step away.",
                payload.app_pattern, used_minutes, payload.daily_minutes
            ))
            .show();
        // 2. Keep the in-app event so any future UI surface (the Quotas
        //    page banner, log viewer, etc.) can react if it's mounted.
        let _ = self.app.emit("quota-exceeded", payload);
    }
}

/// Flush the current state, writing one segment per active field
/// (app, url) for the time elapsed since the last flush. Always
/// runs — the previous "only on change" behaviour dropped every
/// segment after the first one and left a long single-app session
/// recorded as one 10 s blip. Returns `true` iff at least one
/// segment was written.
pub fn flush_now(
    tracker: &UsageTracker,
    store: &dyn FlushStore,
    sink: &dyn EventSink,
    now: i64,
) -> anyhow::Result<bool> {
    let (state, last_change_ts) = tracker.snapshot_for_flush();
    let elapsed = now.saturating_sub(last_change_ts).max(0);
    if elapsed == 0 {
        return Ok(false);
    }

    let mut wrote = false;

    // App segment (if we have an app name).
    if let Some(app) = state.app.as_deref() {
        let seg = UsageSegment {
            id: None,
            kind: "app".into(),
            target: app.to_string(),
            ts: last_change_ts,
            duration_seconds: elapsed,
        };
        store.append(&seg)?;
        wrote = true;

        // Quota: increment by the same delta. The increment function
        // handles the daily reset; we just hand off the elapsed window.
        let today = today_local();
        if let Ok(Some(q)) = store.lookup_active_quota(app) {
            let (new_used, crossed) = store.increment_quota(app, elapsed, &today)?;
            if crossed {
                sink.emit_quota_exceeded(QuotaExceededPayload {
                    app_pattern: q.app_pattern,
                    used_seconds: new_used,
                    daily_minutes: q.daily_minutes,
                });
            }
        }
    }

    // URL segment (if we have a URL).
    if let Some(url) = state.url.as_deref() {
        let seg = UsageSegment {
            id: None,
            kind: "url".into(),
            target: url.to_string(),
            ts: last_change_ts,
            duration_seconds: elapsed,
        };
        store.append(&seg)?;
        wrote = true;
    }

    if wrote {
        tracker.advance_anchor(now);
    }

    Ok(wrote)
}

/// Handle returned by `start`. Drop the handle (or call `stop`) to
/// signal the background task to exit.
pub struct UsageHandle {
    stop_tx: broadcast::Sender<()>,
    join: Option<JoinHandle<()>>,
}

impl UsageHandle {
    pub fn stop(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(j) = self.join.take() {
            // Best-effort: don't block the caller on shutdown.
            j.abort();
        }
    }
}

/// Spawn the background flush task. Wakes every 10 s and flushes
/// only when the snapshot has changed since the last successful
/// flush.
pub fn start(app: AppHandle, pool: DbPool) -> UsageHandle {
    let tracker = UsageTracker::new();
    let store: Arc<dyn FlushStore> = Arc::new(DbStore { pool });
    let sink: Arc<dyn EventSink> = Arc::new(TauriSink { app });

    // Register the tracker so proxy / guard can call into it.
    register(tracker.clone());

    let (stop_tx, _) = broadcast::channel(1);
    let stop_rx = stop_tx.subscribe();
    let tracker_loop = tracker.clone();
    let store_loop = store.clone();
    let sink_loop = sink.clone();

    let join = tauri::async_runtime::spawn(async move {
        let mut tick = interval(Duration::from_secs(10));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut stop_rx = stop_rx;
        loop {
            tokio::select! {
                _ = stop_rx.recv() => break,
                _ = tick.tick() => {
                    let now = now_ts();
                    let _ = flush_now(&tracker_loop, store_loop.as_ref(), sink_loop.as_ref(), now);
                }
            }
        }
    });

    // Companion: count time for apps with active quotas even when
    // they're running in the background (e.g. a game launched through
    // its launcher — the launcher is foreground, the game never is).
    // Skips apps currently in the foreground so the foreground
    // tracker remains the sole writer for those.
    #[cfg(windows)]
    let _bg = crate::engine::background_tracker::start(store.clone(), sink.clone());

    UsageHandle {
        stop_tx,
        join: Some(join),
    }
}

// ─── tracker registry ──────────────────────────────────────────────
//
// The proxy and guard need to call into the current tracker but
// don't have a handle to it. We register a process-global one at
// start() and clear it on drop. Same trick the guard uses for its
// Win32 callback state.

use once_cell::sync::OnceCell;
static TRACKER: OnceCell<UsageTracker> = OnceCell::new();

fn register(t: UsageTracker) {
    // If a prior session left one set (e.g. test-only with the same
    // process), drop it and replace. In production `start` is gated
    // by the active_proxy / active_guard state so this is a no-op.
    let _ = TRACKER.set(t);
}

pub fn current() -> Option<UsageTracker> {
    TRACKER.get().cloned()
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    /// In-memory store that captures the segments written and the
    /// quota increments requested. Lets us assert on the *exact*
    /// observable behavior of `flush_now` without touching sqlite.
    struct MockStore {
        segs: Mutex<Vec<UsageSegment>>,
        increment_count: AtomicI64,
        next_quota: Mutex<Vec<AppQuota>>,
        crossed: AtomicI64,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                segs: Mutex::new(Vec::new()),
                increment_count: AtomicI64::new(0),
                next_quota: Mutex::new(Vec::new()),
                crossed: AtomicI64::new(0),
            }
        }
    }

    impl FlushStore for MockStore {
        fn append(&self, seg: &UsageSegment) -> anyhow::Result<i64> {
            self.segs.lock().unwrap().push(seg.clone());
            Ok(self.segs.lock().unwrap().len() as i64)
        }
        fn increment_quota(
            &self,
            _pattern: &str,
            delta: i64,
            _today: &str,
        ) -> anyhow::Result<(i64, bool)> {
            self.increment_count.fetch_add(1, Ordering::SeqCst);
            let crossed = self.crossed.load(Ordering::SeqCst) > 0;
            Ok((delta, crossed))
        }
        fn lookup_active_quota(&self, pattern: &str) -> anyhow::Result<Option<AppQuota>> {
            let g = self.next_quota.lock().unwrap();
            Ok(g.iter().find(|q| q.app_pattern == pattern).cloned())
        }
        fn list_active_quotas(&self) -> anyhow::Result<Vec<AppQuota>> {
            Ok(self.next_quota.lock().unwrap().clone())
        }
    }

    struct CollectingSink {
        events: Mutex<Vec<QuotaExceededPayload>>,
    }
    impl CollectingSink {
        fn new() -> Self { Self { events: Mutex::new(Vec::new()) } }
    }
    impl EventSink for CollectingSink {
        fn emit_quota_exceeded(&self, payload: QuotaExceededPayload) {
            self.events.lock().unwrap().push(payload);
        }
    }

    #[test]
    fn flush_skips_when_unchanged() {
        let t = UsageTracker::new();
        let store = MockStore::new();
        let sink = CollectingSink::new();
        // Never touched the tracker → no change.
        let wrote = flush_now(&t, &store, &sink, now_ts()).unwrap();
        assert!(!wrote, "no change → nothing written");
        assert!(store.segs.lock().unwrap().is_empty());
    }

    #[test]
    fn flush_writes_one_segment_per_cycle_until_anchor_advances() {
        // Each flush writes app + url segments covering the time since
        // the previous flush. This is what makes a 30 min single-app
        // session show up as 30 min on the Quotas page instead of a
        // single 10 s blip.
        let t = UsageTracker::new();
        t.update(Some("chrome.exe"), Some("https://x.com"), None);
        let store = MockStore::new();
        let sink = CollectingSink::new();

        let mut fake_now = now_ts() + 30;
        let wrote = flush_now(&t, &store, &sink, fake_now).unwrap();
        assert!(wrote);
        let segs = store.segs.lock().unwrap();
        assert_eq!(segs.len(), 2, "app + url segments");
        assert!(segs.iter().any(|s| s.kind == "app" && s.target == "chrome.exe" && s.duration_seconds == 30));
        assert!(segs.iter().any(|s| s.kind == "url" && s.target == "https://x.com" && s.duration_seconds == 30));

        // Second flush 30 s later — same state, but elapsed > 0 so
        // we still write. Anchor advanced to fake_now, so durations
        // measure from there.
        drop(segs);
        fake_now += 30;
        let wrote2 = flush_now(&t, &store, &sink, fake_now).unwrap();
        assert!(wrote2, "second flush must also write — anchor advanced");
        let segs = store.segs.lock().unwrap();
        assert_eq!(segs.len(), 4, "two more segments (app + url)");
        let last_two = &segs[2..];
        assert!(last_two.iter().any(|s| s.kind == "app" && s.target == "chrome.exe" && s.duration_seconds == 30));
        assert!(last_two.iter().any(|s| s.kind == "url" && s.target == "https://x.com" && s.duration_seconds == 30));
    }

    #[test]
    fn flush_app_transition_writes_two_segments() {
        // The "integration" case from the brief: drive A→B, flush, two
        // segments. The first flush covers the A→B transition (the
        // *change* to B is what marks changed). A second flush with
        // no further change is a no-op.
        let t = UsageTracker::new();
        let store = MockStore::new();
        let sink = CollectingSink::new();

        // t0: begin observing app A. We use update_at so the change
        // timestamp is deterministic — the live `update` reads the
        // wall clock, which would couple test expectations to scheduler
        // jitter.
        let t0 = 1_000_000i64;
        t.update_at(Some("chrome.exe"), None, None, t0);
        let t_switch = t0 + 20;
        assert!(flush_now(&t, &store, &sink, t_switch).unwrap());
        // The flush wrote the chrome.exe segment covering the 20s.

        let segs = store.segs.lock().unwrap().clone();
        assert_eq!(segs.len(), 1, "first flush: 1 app segment");
        assert_eq!(segs[0].target, "chrome.exe");
        assert_eq!(segs[0].duration_seconds, 20);

        // Switch to B at the same instant as the flush — the change
        // happened because the user clicked. The frame after the flush
        // is the new window's anchor.
        t.update_at(Some("slack.exe"), None, None, t_switch);
        // 15s later flush again.
        let t_flush_b = t_switch + 15;
        assert!(flush_now(&t, &store, &sink, t_flush_b).unwrap());
        let segs = store.segs.lock().unwrap().clone();
        assert_eq!(segs.len(), 2, "after transition: 2 app segments");
        assert!(segs.iter().any(|s| s.target == "chrome.exe" && s.duration_seconds == 20));
        assert!(segs.iter().any(|s| s.target == "slack.exe" && s.duration_seconds == 15));
    }

    #[test]
    fn flush_increments_matched_quota_and_emits_on_cross() {
        let t = UsageTracker::new();
        let store = MockStore::new();
        let sink = CollectingSink::new();

        // Pre-arm the mock with a quota for chrome.exe (60 min/day).
        store.next_quota.lock().unwrap().push(AppQuota {
            id: Some(1),
            app_pattern: "chrome.exe".into(),
            daily_minutes: 60,
            used_seconds: 0,
            used_day: today_local(),
            active: true,
            created_at: 0,
        });
        // Tell the mock its mock increment will report crossed=true.
        store.crossed.store(1, Ordering::SeqCst);

        t.update(Some("chrome.exe"), None, None);
        let wrote = flush_now(&t, &store, &sink, now_ts() + 45).unwrap();
        assert!(wrote);

        assert_eq!(store.increment_count.load(Ordering::SeqCst), 1);
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1, "crossed threshold → emit exactly once");
        assert_eq!(events[0].app_pattern, "chrome.exe");
        assert_eq!(events[0].used_seconds, 45);
        assert_eq!(events[0].daily_minutes, 60);
    }

    #[test]
    fn flush_no_quota_means_no_increment_and_no_event() {
        let t = UsageTracker::new();
        let store = MockStore::new();
        let sink = CollectingSink::new();

        t.update(Some("notepad.exe"), None, None);
        let wrote = flush_now(&t, &store, &sink, now_ts() + 10).unwrap();
        assert!(wrote);
        assert_eq!(store.increment_count.load(Ordering::SeqCst), 0);
        assert!(sink.events.lock().unwrap().is_empty());
    }
}
