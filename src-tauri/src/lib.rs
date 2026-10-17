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

/// `kadr --self-check <out.json>`: finds the bundled helpers and geobase the
/// way the app does, writes what it found as JSON and exits — no window.
/// Used by the installer smoke tests on every OS.
fn self_check(package_info: &tauri::PackageInfo, out: Option<&str>) -> i32 {
    use kadr_core::services::indexer::Helpers;
    let resources = tauri::utils::platform::resource_dir(package_info, &tauri::Env::default()).ok();
    let helpers_dir = resources.as_ref().map(|d| d.join("helpers")).filter(|d| d.is_dir());
    let (_, tools) = Helpers::detect(&kadr_core::settings::AppSettings::default(), helpers_dir.as_deref());
    let geobase = resources
        .as_ref()
        .map(|d| d.join(GEOBASE_FILE))
        .filter(|p| p.is_file())
        .map(|p| OfflineGeocoder::open(&p).map(|g| g.info().clone()).map_err(|e| e.to_string()));
    let all_bundled = tools.iter().all(|t| {
        matches!(
            t.state,
            kadr_core::services::tools::ToolState::Available {
                source: kadr_core::services::tools::ToolSource::Bundled,
                ..
            }
        )
    });
    let ok = all_bundled && matches!(geobase, Some(Ok(_)));
    let report = serde_json::json!({
        "ok": ok,
        "resource_dir": resources,
        "helpers_dir": helpers_dir,
        "tools": tools,
        "geobase": match &geobase {
            Some(Ok(info)) => serde_json::json!(info),
            Some(Err(e)) => serde_json::json!({ "error": e }),
            None => serde_json::Value::Null,
        },
    });
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    match out {
        Some(path) => {
            if std::fs::write(path, &text).is_err() {
                return 2;
            }
        }
        None => println!("{text}"),
    }
    i32::from(!ok)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--self-check") {
        std::process::exit(self_check(context.package_info(), args.get(2).map(String::as_str)));
    }
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
            commands::library::set_manual_place,
            commands::library::set_manual_date,
            commands::sources::get_folders,
            commands::sources::add_source_roots,
            commands::sources::remove_source_root,
            commands::sources::set_destination_root,
            commands::scan::start_scan,
            commands::scan::cancel_scan,
            commands::scan::get_scan_status,
        ])
        .run(context);
    if let Err(err) = result {
        eprintln!("kadr: failed to start: {err}");
        std::process::exit(1);
    }
}
