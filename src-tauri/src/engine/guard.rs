//! Windows accessibility hook (SetWinEventHook) that closes or hides
//! blocked app windows when their process gains focus.
//!
//! Activated only while a focus session is running with at least one
//! app-path or title rule. `UnhookWinEvent` is resolved at runtime via
//! `GetProcAddress` so the binary still loads on hosts whose
//! `user32.dll` lacks the export.

#![cfg(windows)]

use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::{AppHandle, Emitter};
use crate::storage::block_rule::BlockRule;
use crate::engine::rule_match::{match_str, matches_app, matches_title};

#[derive(Clone)]
struct CallbackState {
    app: AppHandle,
    rules: Arc<RwLock<Vec<BlockRule>>>,
}

// Mutex<Option<>> so start() can be called repeatedly across focus
// sessions. OnceCell would panic / Err on the second call.
static CALLBACK_STATE: std::sync::Mutex<Option<CallbackState>> = std::sync::Mutex::new(None);

// Resolve UnhookWinEvent at runtime via GetProcAddress so the linker
// never pulls the symbol into test binaries. The symbol name only ever
// appears as a C string literal — never as a Rust identifier — so even
// on hosts whose user32.dll lacks the export, the binary still loads
// (STATUS_ENTRYPOINT_NOT_FOUND avoided).
//
// LoadLibraryA / GetProcAddress are declared as raw FFI instead of via
// windows::Win32::System::LibraryLoader so we don't need the
// Win32_System_LibraryLoader cargo feature. Both symbols are in
// kernel32.dll, which is loaded by every Windows process.
mod unhook {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::UI::Accessibility::HWINEVENTHOOK;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut c_void;
        fn GetProcAddress(
            h_module: *mut c_void,
            lp_proc_name: *const u8,
        ) -> Option<unsafe extern "system" fn() -> isize>;
    }

    static FN: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

    /// Call UnhookWinEvent, lazy-resolving the symbol. Silently no-op
    /// if resolution fails (test hosts without the export).
    pub unsafe fn call(hook: HWINEVENTHOOK) {
        let mut p = FN.load(Ordering::Acquire);
        if p.is_null() {
            let module = LoadLibraryA(b"user32.dll\0".as_ptr());
            if module.is_null() { return; }
            let proc = match GetProcAddress(module, b"UnhookWinEvent\0".as_ptr()) {
                Some(p) => p,
                None => return,
            };
            p = proc as *mut c_void;
            FN.store(p, Ordering::Release);
        }
        let f: unsafe extern "system" fn(HWINEVENTHOOK) -> BOOL =
            std::mem::transmute(p);
        let _ = f(hook);
    }
}

pub struct GuardHandle {
    hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
}

// HWINEVENTHOOK is a process-global handle; UnhookWinEvent is thread-safe.
// Tauri's State requires Send + Sync, and the windows crate doesn't auto-impl
// these for raw handles.
unsafe impl Send for GuardHandle {}
unsafe impl Sync for GuardHandle {}

impl GuardHandle {
    pub fn stop(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        unsafe { unhook::call(self.hook); }
        Ok(())
    }
}

pub fn start(
    app: AppHandle,
    rules: Arc<RwLock<Vec<BlockRule>>>,
) -> Result<GuardHandle, Box<dyn std::error::Error + Send + Sync>> {
    use windows::Win32::UI::Accessibility::{
        SetWinEventHook, HWINEVENTHOOK,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    };

    let state = CallbackState { app, rules };
    // Overwrite any prior state so successive start() calls (across
    // focus sessions) get fresh AppHandle/rules references.
    *CALLBACK_STATE.lock().unwrap() = Some(state);

    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };

    // SetWinEventHook with WINEVENT_OUTOFCONTEXT only delivers
    // callbacks when the calling process pumps Win32 messages. Tauri
    // doesn't, so the hook installs (handle is non-NULL) but never
    // fires — leaving app segments absent from `usage_log` even after
    // a real 30 min focus session. A 2 s poll is bulletproof and
    // cheap; the callback is kept as a best-effort accelerator.
    spawn_foreground_poll();

    Ok(GuardHandle { hook: HWINEVENTHOOK(hook.0) })
}

/// 2 s foreground-poll: ensures app usage segments get written even
/// when the WinEventHook callback is starved. Spawns a tokio task
/// that reads `GetForegroundWindow`, resolves the exe basename and
/// pushes it through the usage tracker.
fn spawn_foreground_poll() {
    use std::time::Duration;
    use tokio::time::{interval, MissedTickBehavior};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;

            let hwnd = unsafe { GetForegroundWindow() };
            let mut pid: u32 = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == 0 { continue; }

            let path = match get_process_path(pid) {
                Some(p) => p,
                None => continue,
            };
            let exe_name = extract_exe_name(&path)
                .map(|n| n.to_lowercase())
                .unwrap_or_else(|| path.to_lowercase());
            let title = get_window_title(hwnd);

            if let Some(t) = crate::engine::app_usage::current() {
                t.update(Some(&exe_name), None, Some(&title));
            }
        }
    });
}

unsafe extern "system" fn foreground_callback(
    _hwineventhook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    _hwnd: windows::Win32::Foundation::HWND,
    _idobject: i32,
    _idchild: i32,
    _idevent_thread: u32,
    _ms_event_time: u32,
) {
    let state = match CALLBACK_STATE.lock().unwrap().clone() {
        Some(s) => s,
        None => return,
    };
    let rules = state.rules.clone();
    let app = state.app.clone();

    let hwnd = windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
    let mut pid: u32 = 0;
    windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 { return; }
    let path = match get_process_path(pid) {
        Some(p) => p,
        None => return,
    };
    let title = get_window_title(hwnd);

    // Use blocking_read on the tokio RwLock — we're on a non-tokio
    // Win32 thread, so blocking is acceptable.
    let rules_snapshot = rules.blocking_read().clone();
    if let Some(matched) = check_foreground_for_rules(&path, &title, &rules_snapshot) {
        let _ = app.emit("window-guard-hit", matched);
    }
    // Usage tracking: record the new foreground window so the engine
    // can accumulate app + title usage in the background. Strip the
    // directory to the exe basename — `app_quota::increment_used` and
    // `flush_now::lookup_active_quota` match on `app_pattern` exactly,
    // and the UI shows users basenames like "Discord.exe" /
    // "YuanShen.exe" to fill in. Passing the full path made the lookup
    // fail silently for every quota entry.
    if let Some(t) = crate::engine::app_usage::current() {
        // Lowercase the basename: Windows file names are
        // case-insensitive but our quota table matches exactly, so
        // a user-typed "notepad.exe" needs to land on the OS-reported
        // "Notepad.exe".
        let exe_name = extract_exe_name(&path)
            .map(|n| n.to_lowercase())
            .unwrap_or_else(|| path.to_lowercase());
        let _ = t.update(Some(&exe_name), None, Some(&title));
    }
}

fn get_process_path(pid: u32) -> Option<String> {
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::Foundation::CloseHandle;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16(&buffer[..size as usize]).ok()?;
        Some(path)
    }
}

fn get_window_title(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16(&buffer[..len as usize]).unwrap_or_default()
}

pub fn extract_exe_name(path: &str) -> Option<String> {
    let name = std::path::Path::new(path).file_name()?;
    Some(name.to_string_lossy().to_string())
}

pub fn matches_active_rule(rules: &[BlockRule], exe_path: &str, title: &str) -> bool {
    // Whitelist wins: any active whitelist rule matching the process path
    // or window title disables blocking entirely for this foreground window.
    if rules
        .iter()
        .filter(|r| r.active && r.kind == "whitelist")
        .any(|r| {
            match_str(exe_path, &r.pattern, &r.match_type)
                || match_str(title, &r.pattern, &r.match_type)
        })
    {
        return false;
    }
    rules.iter().filter(|r| r.active).any(|r| match r.kind.as_str() {
        "app_path" => matches_app(r, exe_path),
        "title" => matches_title(r, title),
        _ => false,
    })
}

pub fn check_foreground_for_rules(exe_path: &str, title: &str, rules: &[BlockRule]) -> Option<String> {
    if matches_active_rule(rules, exe_path, title) {
        Some(exe_path.to_string())
    } else {
        None
    }
}
