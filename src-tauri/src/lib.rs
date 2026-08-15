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
        .setup(|app| {
            use tauri::Manager;
            let app_data = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&app_data).ok();
            // Crash-recovery: if a prior session set the proxy and was killed
            // before stop_proxy could run, the marker file is still on disk.
            // Detect it and restore the original proxy state before we start
            // serving any UI.
            match storage::proxy_marker::reap(&app_data) {
                Ok(Some(marker)) => eprintln!(
                    "[setup] recovered proxy state from prior crashed session (was {}:{} enabled)",
                    marker.snapshot.server.as_deref().unwrap_or("<none>"),
                    marker.snapshot.enable,
                ),
                Ok(None) => {}
                Err(e) => eprintln!("[setup] proxy marker reap failed: {}", e),
            }
            let pool = storage::db::open_pool(&app_data.join("quietly.db"))
                .expect("open db");
            let state = ipc::commands::AppState::new(pool, app_data.clone());
            app.manage(state);
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            use tauri::{Manager, WindowEvent};
            // Clicking the X — tear down any focus session first so the
            // system proxy isn't left pointing at a dead process. We clone
            // the AppHandle (cheap, refcounted) and re-fetch AppState inside
            // the spawned future so the &Window borrow doesn't escape.
            if let WindowEvent::CloseRequested { .. } = event {
                let app_handle = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let inner = app_handle.state::<ipc::commands::AppState>();
                    ipc::commands::graceful_shutdown(&inner).await;
                    app_handle.exit(0);
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::commands::list_block_rules,
            ipc::commands::create_block_rule,
            ipc::commands::update_block_rule,
            ipc::commands::delete_block_rule,
            ipc::commands::set_block_rule_active,
            ipc::commands::list_schedules,
            ipc::commands::create_schedule,
            ipc::commands::update_schedule,
            ipc::commands::delete_schedule,
            ipc::commands::toggle_schedule,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}