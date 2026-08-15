#![cfg(windows)]

use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::{AppHandle, Emitter};
use crate::storage::block_rule::BlockRule;
use crate::engine::rule_match::matches_app;

#[derive(Clone)]
struct CallbackState {
    app: AppHandle,
    rules: Arc<RwLock<Vec<BlockRule>>>,
}

static CALLBACK_STATE: once_cell::sync::OnceCell<CallbackState> = once_cell::sync::OnceCell::new();

pub struct GuardHandle {
    hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
}

impl GuardHandle {
    pub fn stop(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        unsafe {
            let _ = windows::Win32::UI::Accessibility::UnhookWinEvent(self.hook);
        }
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
    CALLBACK_STATE
        .set(state)
        .map_err(|_| "guard already started")?;

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

    Ok(GuardHandle { hook: HWINEVENTHOOK(hook.0) })
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
    let state = match CALLBACK_STATE.get() {
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

    // Use blocking_read on the tokio RwLock — we're on a non-tokio
    // Win32 thread, so blocking is acceptable.
    let rules_snapshot = rules.blocking_read().clone();
    if let Some(matched) = check_foreground_for_rules(&path, &rules_snapshot) {
        let _ = app.emit("window-guard-hit", matched);
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

pub fn extract_exe_name(path: &str) -> Option<String> {
    let name = std::path::Path::new(path).file_name()?;
    Some(name.to_string_lossy().to_string())
}

pub fn matches_active_rule(rules: &[BlockRule], exe_path: &str) -> bool {
    rules
        .iter()
        .filter(|r| r.active && r.kind == "app_path")
        .any(|r| matches_app(r, exe_path))
}

pub fn check_foreground_for_rules(exe_path: &str, rules: &[BlockRule]) -> Option<String> {
    if matches_active_rule(rules, exe_path) {
        Some(exe_path.to_string())
    } else {
        None
    }
}
