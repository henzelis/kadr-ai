mod app_state;
mod commands;
mod errors;
mod thumbs;

use kadr_core::persistence::Db;
use kadr_core::services::geocoder::OfflineGeocoder;
use tauri::Manager;

use crate::app_state::AppState;

const DB_FILE: &str = "kadr.sqlite3";
const GEOBASE_FILE: &str = "geobase.sqlite3";

/// An imported geobase in app data wins over one shipped with the app.
fn load_geocoder(app: &tauri::App, imported: &std::path::Path) -> Option<OfflineGeocoder> {
    let bundled = app.path().resource_dir().ok().map(|d| d.join(GEOBASE_FILE));
    [Some(imported.to_path_buf()), bundled]
        .into_iter()
        .flatten()
        .filter(|p| p.is_file())
        .find_map(|p| {
            OfflineGeocoder::open(&p)
                .inspect_err(|e| eprintln!("kadr: geobase {} not loaded: {e}", p.display()))
                .ok()
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("thumb", thumbs::handle)
        .setup(|app| {
            // The index lives in the app data directory, never next to originals.
            let data_dir = app.path().app_data_dir()?;
            let path = data_dir.join(DB_FILE);
            let db = Db::open(&path)?;
            let geobase_path = data_dir.join(GEOBASE_FILE);
            let geocoder = load_geocoder(app, &geobase_path);
            let interrupted = db.mark_interrupted_scans()?;
            if interrupted > 0 {
                eprintln!(
                    "kadr: {interrupted} scan(s) from a previous session marked as interrupted"
                );
            }
            let helpers_dir = app
                .path()
                .resource_dir()
                .ok()
                .map(|d| d.join("helpers"))
                .filter(|d| d.is_dir());
            let thumbs_dir = app.path().app_cache_dir()?.join("thumbnails");
            let settings = db.load_settings()?;
            app.manage(AppState::new(
                db,
                path,
                geobase_path,
                geocoder,
                helpers_dir,
                thumbs_dir,
            ));
            // Probing helpers takes a moment; do not delay the window for it.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                handle.state::<AppState>().rebuild_thumbnailer(&settings);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_tool_status,
            commands::geobase::get_geobase_status,
            commands::geobase::import_geobase,
            commands::library::get_library_summary,
            commands::library::query_media,
            commands::library::get_media_details,
            commands::library::get_section_counts,
            commands::sources::get_folders,
            commands::sources::add_source_roots,
            commands::sources::remove_source_root,
            commands::sources::set_destination_root,
            commands::scan::start_scan,
            commands::scan::cancel_scan,
            commands::scan::get_scan_status,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("kadr: failed to start: {err}");
        std::process::exit(1);
    }
}
