#![cfg(windows)]

//! Background-app quota tracker.
//!
//! The foreground tracker only counts time when an app's window is the
//! system foreground — a game launched through a launcher (YuanShen
//! via miHoYo Launcher, Discord via Steam, etc.) never becomes the
//! foreground and so its time is never counted. This module
//! complements the foreground tracker by scanning every running
//! process every 5 s and counting time for apps with an active quota
//! regardless of which window is in front.
//!
//! To avoid double-counting time that's already captured by the
//! foreground tracker, we skip any app that is currently the
//! foreground window — the foreground tracker is responsible for
//! that window's time.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

use crate::engine::app_usage::{
    current, now_ts, today_local, AppQuotaReexport as AppQuota, EventSink, FlushStore,
    QuotaExceededPayload, UsageSegmentReexport as UsageSegment,
};

/// Spawn the background tracker task. The handle is detached — the
/// loop runs for the lifetime of the app. Tauri's runtime shuts down
/// with the process.
pub fn start(store: Arc<dyn FlushStore>, sink: Arc<dyn EventSink>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;

            // Pull live active quotas from the store each tick — cheap
            // (sqlite, indexed) and lets the user toggle quotas without
            // restarting the engine.
            let quotas = match store.list_active_quotas() {
                Ok(q) => q,
                Err(_) => continue,
            };
            if quotas.is_empty() {
                continue;
            }

            // Single EnumProcesses call per tick. Each PID's exe path
            // resolves via QueryFullProcessImageNameW (which works
            // against PPL games with PROCESS_QUERY_LIMITED_INFORMATION
            // = 0x1000). Failures (access denied, etc.) are silently
            // skipped — the app just won't count this tick.
            let running = match enumerate_running_exes() {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Foreground app name — empty if no tracker is registered
            // yet (e.g. focus session hasn't started). In that case
            // nothing is foreground, so we don't skip anything.
            let foreground = current()
                .map(|t| t.snapshot().app)
                .flatten()
                .unwrap_or_default();

            for quota in &quotas {
                if !running.contains(&quota.app_pattern) {
                    continue;
                }
                if foreground == quota.app_pattern {
                    continue;
                }
                tick_one(&*store, &*sink, quota);
            }
        }
    });
}

fn tick_one(store: &dyn FlushStore, sink: &dyn EventSink, quota: &AppQuota) {
    const TICK_SECONDS: i64 = 5;
    let today = today_local();

    let seg = UsageSegment {
        id: None,
        kind: "app".into(),
        target: quota.app_pattern.clone(),
        ts: now_ts(),
        duration_seconds: TICK_SECONDS,
    };
    let _ = store.append(&seg);

    if let Ok((new_used, crossed)) =
        store.increment_quota(&quota.app_pattern, TICK_SECONDS, &today)
    {
        if crossed {
            sink.emit_quota_exceeded(QuotaExceededPayload {
                app_pattern: quota.app_pattern.clone(),
                used_seconds: new_used,
                daily_minutes: quota.daily_minutes,
            });
        }
    }
}

/// Snapshot every running process's exe basename. Returns a set so
/// lookups are O(1). PIDs we can't open (system, protected) are
/// silently dropped — they don't have a quota pattern anyway.
fn enumerate_running_exes() -> anyhow::Result<HashSet<String>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // 8 KiB → 2048 PIDs. Typical Windows hosts have ~150 processes
    // so this is rarely exceeded; the unbounded retry below covers it.
    let mut pids = vec![0u32; 2048];
    let mut bytes_returned: u32 = 0;
    let needed = unsafe {
        EnumProcesses(
            pids.as_mut_ptr(),
            (pids.len() * std::mem::size_of::<u32>()) as u32,
            &mut bytes_returned,
        )
    };
    if needed.is_err() {
        // Almost always means "buffer too small". Re-try with the
        // exact size the OS would have needed.
        let count = bytes_returned as usize / std::mem::size_of::<u32>();
        pids.resize(count, 0);
        let _ = unsafe {
            EnumProcesses(
                pids.as_mut_ptr(),
                (pids.len() * std::mem::size_of::<u32>()) as u32,
                &mut bytes_returned,
            )
        };
    }
    let count = bytes_returned as usize / std::mem::size_of::<u32>();
    pids.truncate(count);

    let mut set = HashSet::with_capacity(count);
    for &pid in &pids {
        if pid == 0 {
            continue;
        }
        let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        {
            Ok(h) => h,
            Err(_) => continue,
        };
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let qresult = unsafe {
            QueryFullProcessImageNameW(
                handle,
                windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };
        let _ = unsafe { CloseHandle(handle) };
        let path = match qresult {
            Ok(_) => String::from_utf16(&buffer[..size as usize]).unwrap_or_default(),
            Err(_) => continue,
        };
        if let Some(name) = std::path::Path::new(&path).file_name() {
            // Lowercase so a user-typed "notepad.exe" matches the
            // OS-reported "Notepad.exe". Windows filenames are
            // case-insensitive; our quota store is case-sensitive.
            set.insert(name.to_string_lossy().to_lowercase());
        }
    }
    Ok(set)
}

// ─── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_returns_current_process() {
        // Sanity: enumerating must return at least our own test binary
        // (or some process whose exe we can open). If this fails the
        // Windows API surface has shifted.
        let set = enumerate_running_exes().expect("enum ok");
        assert!(!set.is_empty(), "should find at least one process");
    }
}