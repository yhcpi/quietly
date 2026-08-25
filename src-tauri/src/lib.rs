pub mod engine;
pub mod storage;
pub mod ipc;
pub mod tray;

/// Check that the Microsoft Edge WebView2 Runtime is installed.
///
/// On most Windows 10 1809+ / Windows 11 machines WebView2 is
/// preinstalled (it ships with Edge). On LTSC images, locked-down
/// enterprise builds, or any host where Edge was removed, the
/// runtime is missing and the Tauri window fails to create with a
/// generic "Could not load webview" error. Probe the registry so we
/// can give the user an actionable hint before that happens.
///
/// In release builds stderr is discarded (`windows_subsystem =
/// "windows"`), so this also pops a native MessageBox and writes a
/// diagnostics file under the user's temp dir — otherwise the user
/// gets a silent crash with no clue what to fix.
fn warn_if_webview2_missing() {
    use winreg::enums::KEY_READ;
    use winreg::RegKey;

    const WEBVIEW2_GUID: &str =
        r"{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const SUBKEY: &str =
        r"Software\Microsoft\EdgeUpdate\Clients";
    const INSTALL_URL: &str =
        "https://developer.microsoft.com/microsoft-edge/webview2/";

    // Two-tier check: registry first (the canonical Edge Update
    // bookkeeping), then file existence (covers machines where WebView2
    // is installed but the Edge Update agent never ran, e.g. a one-off
    // zip extraction of the Evergreen Runtime, or a locked-down image
    // where the update service is disabled). If EITHER shows WebView2
    // is present, we treat it as installed and skip the warning.
    for hive in [winreg::enums::HKEY_LOCAL_MACHINE, winreg::enums::HKEY_CURRENT_USER] {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(SUBKEY, KEY_READ) else {
            continue;
        };
        if root.open_subkey(WEBVIEW2_GUID).is_ok() {
            return;
        }
    }
    // File fallback: the Evergreen Runtime always installs
    // `msedgewebview2.exe` under one of these well-known paths. The
    // System32 path is the Windows-shipped location on Windows 11
    // (version 150+); the Program Files path is the standalone
    // Evergreen installer.
    let candidates: &[&str] = &[
        r"C:\Windows\System32\Microsoft-Edge-WebView\msedgewebview2.exe",
        r"C:\Windows\System32\msedgewebview2.exe",
        r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
    ];
    if candidates.iter().any(|p| std::path::Path::new(p).exists()) {
        return;
    }

    let message = format!(
        "Microsoft Edge WebView2 Runtime not detected.\n\
         \n\
         Quietly needs WebView2 to display its window. It is\n\
         preinstalled with Microsoft Edge on Windows 10 1809+ /\n\
         Windows 11. If Edge is not installed (or you removed it),\n\
         download the Evergreen Runtime from:\n\
         \n\
         {INSTALL_URL}\n\
         \n\
         After installing, relaunch Quietly."
    );

    // stderr (only visible in debug builds)
    eprintln!("[setup] FATAL: WebView2 missing. {message}");

    // diagnostics file under temp — survives a silent release crash
    if let Some(tmp) = std::env::var_os("TEMP").or_else(|| std::env::var_os("TMP")) {
        let path = std::path::PathBuf::from(tmp).join("quietly-webview2-missing.txt");
        let _ = std::fs::write(&path, &message);
    }

    // native message box so the user actually sees something in release
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONWARNING, MB_OK,
        };
        let body: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
        let title = "Quietly — WebView2 Runtime required";
        let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(body.as_ptr()),
                PCWSTR(title_wide.as_ptr()),
                MB_ICONWARNING | MB_OK,
            );
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            use tauri::Manager;
            // Proxy debug log defaults to off — opt in with
            // QUIETLY_PROXY_DEBUG=1 (or =true). Reads once at startup;
            // toggling the env var after this point has no effect.
            engine::proxy::init_proxy_debug_log();
            warn_if_webview2_missing();
            let app_data = match app.path().app_data_dir() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[setup] FATAL: cannot resolve app data dir: {}", e);
                    eprintln!("[setup] Hint: %APPDATA% must be set and writable for the current user.");
                    return Err(e.into());
                }
            };
            if let Err(e) = std::fs::create_dir_all(&app_data) {
                eprintln!("[setup] FATAL: cannot create {}: {}", app_data.display(), e);
                return Err(e.into());
            }

            // ─── Crash / exit cleanup ────────────────────────────────
            //
            // Three layers, all run unconditionally on every launch:
            //
            //  1. Reap legacy marker file. Old binaries wrote HKCU
            //     ProxyServer pointing at our local proxy and a
            //     snapshot to restore. If we find one, restore the
            //     original proxy state and delete the marker.
            //
            //  2. Rewrite PAC file to the "DIRECT everything" safe
            //     default. This is the new crash-safe cleanup path.
            //     The PAC file is always a valid JS function that
            //     returns DIRECT for any unblocked host — so even if
            //     the file is stale, the user's browsing keeps working.
            //
            //  3. Clear AutoConfigURL. If the previous session died
            //     mid-flight, AutoConfigURL may still be pointing at
            //     our PAC file. Clearing it means browsers fall back
            //     to their default (DIRECT) without our involvement.
            //
            // Note: there is no longer a "HKCU ProxyServer points at
            // us" orphan audit — the listener port is kernel-assigned
            // (see `proxy::start`) so we cannot recognize our own
            // old port without the marker file, and the marker-based
            // recovery already covers the crash case.
            match storage::proxy_marker::reap(&app_data) {
                Ok(Some(marker)) => eprintln!(
                    "[setup] recovered legacy proxy state (was {}:{} enabled)",
                    marker.snapshot.server.as_deref().unwrap_or("<none>"),
                    marker.snapshot.enable,
                ),
                Ok(None) => {}
                Err(e) => eprintln!("[setup] proxy marker reap failed: {}", e),
            }
            // PAC-safe-default: always write a DIRECT-only PAC, then
            // clear AutoConfigURL. Order matters: write the file
            // first so a re-read between calls still finds a valid
            // PAC, then drop the URL.
            if let Err(e) = engine::pac::write_safe_default(&app_data) {
                eprintln!("[setup] failed to write safe-default PAC: {}", e);
            }
            if let Err(e) = engine::proxy_state::clear_pac_url() {
                eprintln!("[setup] failed to clear AutoConfigURL: {}", e);
            }
            let db_path = app_data.join("quietly.db");
            let pool = match storage::db::open_pool(&db_path) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[setup] FATAL: cannot open db at {}: {}", db_path.display(), e);
                    return Err(e.into());
                }
            };
            let state = ipc::commands::AppState::new(pool, app_data.clone());
            app.manage(state);
            tray::setup(app.handle())?;

            // Start usage tracking for the lifetime of the app, not just
            // during focus sessions. Quota enforcement needs a running
            // tracker to read foreground / enumerate background apps;
            // without this, opening an app for 2 min while Quietly is
            // sitting in the tray (no focus session active) yields a
            // 0-min quota. Idempotent — start_focus can also call it
            // during auto-block bring-up.
            //
            // `app_usage::start` uses tauri::async_runtime::spawn
            // internally (lazy-initializes a tokio runtime), so this
            // works from any thread context — including setup, which
            // runs on the main thread before any reactor exists.
            let state_ref = app.state::<ipc::commands::AppState>();
            if let Err(e) = state_ref.start_app_usage(app.handle().clone()) {
                eprintln!("[setup] failed to start app usage tracker: {}", e);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::WindowEvent;
            // Clicking the X hides the window to the system tray instead
            // of quitting. The focus session / proxy / guard all keep
            // running so background tracking and any active focus block
            // remain effective — the user can come back to the window
            // via the tray menu's "Show" item. The tray "Quit" menu
            // item is the only path that actually tears down state and
            // exits (see tray.rs).
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::commands::list_block_rules,
            ipc::commands::create_block_rule,
            ipc::commands::update_block_rule,
            ipc::commands::delete_block_rule,
            ipc::commands::set_block_rule_active,
            ipc::commands::get_stats,
            ipc::commands::needs_first_run_ack,
            ipc::commands::ack_first_run,
            ipc::commands::list_block_hits,
            ipc::commands::log_block_hit,
            ipc::commands::list_sessions,
            ipc::commands::start_focus,
            ipc::commands::pause_focus,
            ipc::commands::resume_focus,
            ipc::commands::cancel_focus,
            ipc::commands::start_proxy,
            ipc::commands::stop_proxy,
            ipc::commands::start_guard,
            ipc::commands::stop_guard,
            // 6-feature additions: whitelist, lock plan, app quota, usage stats.
            ipc::commands::list_whitelist,
            ipc::commands::create_whitelist,
            ipc::commands::delete_whitelist,
            ipc::commands::toggle_whitelist,
            ipc::commands::set_session_locked,
            ipc::commands::is_session_locked,
            ipc::commands::list_quotas,
            ipc::commands::create_quota,
            ipc::commands::update_quota,
            ipc::commands::delete_quota,
            ipc::commands::set_app_quota_active,
            ipc::commands::get_quota_status,
            ipc::commands::get_today_usage,
            ipc::commands::get_usage_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}