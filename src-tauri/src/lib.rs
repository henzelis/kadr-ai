mod app_state;
mod commands;
mod errors;

use kadr_core::persistence::Db;
use tauri::Manager;

use crate::app_state::AppState;

const DB_FILE: &str = "kadr.sqlite3";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            // The index lives in the app data directory, never next to originals.
            let path = app.path().app_data_dir()?.join(DB_FILE);
            let db = Db::open(&path)?;
            app.manage(AppState::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_tool_status,
            commands::library::get_library_summary,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("kadr: failed to start: {err}");
        std::process::exit(1);
    }
}
