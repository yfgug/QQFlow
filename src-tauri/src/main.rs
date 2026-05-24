// QQFlow-Rust: QQ Chat Export Tool
// Tauri desktop application replacing the original Electron + Python stack

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db_scan;
mod export_chat;
mod analysis;
mod message_parser;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::scan_databases,
            commands::extract_key,
            commands::get_key_status,
            commands::start_export,
            commands::clear_msg_store,
            commands::cancel_export,
            commands::get_export_status,
            commands::get_analysis_progress,
            commands::get_csv_progress,
            commands::export_csv,
            commands::analyze_group,
            commands::analyze_private,
            commands::debug_db_schema,
            commands::save_key,
            commands::load_keys,
            commands::clear_key,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
