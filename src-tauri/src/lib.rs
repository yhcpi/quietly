pub mod engine;
pub mod storage;
pub mod ipc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            let app_data = app.path().app_data_dir().expect("no app data dir");
            std::fs::create_dir_all(&app_data).ok();
            let pool = storage::db::open_pool(&app_data.join("quietly.db"))
                .expect("open db");
            let state = ipc::commands::AppState::new(pool);
            app.manage(state);
            Ok(())
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