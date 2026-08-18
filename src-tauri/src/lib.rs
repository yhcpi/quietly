pub mod engine;
pub mod storage;
pub mod ipc;
pub mod tray;

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
            let app_data = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&app_data).ok();

            // ─── Crash / exit cleanup ────────────────────────────────
            //
            // Three layers, all run unconditionally on every launch:
            //
            //  1. Reap legacy marker file. Old binaries wrote HKCU
            //     ProxyServer=127.0.0.1:47820 and a snapshot to restore.
            //     If we find one, restore the original proxy state and
            //     delete the marker.
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
            //  4. Orphan audit: HKCU ProxyServer might still be
            //     pointing at our port from a pre-PAC binary. Clear
            //     it for the same reason.
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
            // Orphan audit (legacy HKCU proxy).
            if let Ok(current) = engine::proxy_state::read_current() {
                let orphan = current.server.as_deref()
                    == Some("127.0.0.1:47820") && current.enable;
                if orphan {
                    eprintln!(
                        "[setup] WARNING: legacy HKCU proxy still at 127.0.0.1:47820 — clearing."
                    );
                    let _ = engine::proxy_state::clear_proxy();
                    // Belt-and-suspenders: also clear the per-connection
                    // PAC field, in case a pre-PAC binary left one set
                    // on the binary blob. This is the same path as
                    // stop_proxy_inner — empty string clears
                    // INTERNET_PER_CONN_AUTOCONFIG_URL.
                    engine::proxy_state::clear_pac_url_per_connection();
                    // Same WinINET cache invalidation as the focus
                    // session path: stale ProxyEnable=1 from a pre-PAC
                    // binary is meaningless after we clear it, but
                    // running browsers only notice when the broadcast
                    // tells them to re-read the registry.
                    engine::proxy_state::notify_proxy_changed();
                }
            }
            let pool = storage::db::open_pool(&app_data.join("quietly.db"))
                .expect("open db");
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
            ipc::commands::get_settings,
            ipc::commands::set_settings,
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