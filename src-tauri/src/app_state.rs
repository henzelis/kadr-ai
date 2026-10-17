use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use kadr_core::persistence::Db;
use kadr_core::services::geocoder::OfflineGeocoder;
use kadr_core::services::indexer::{Helpers, ScanProgress};
use kadr_core::services::thumbnail::Thumbnailer;
use kadr_core::settings::AppSettings;

use crate::errors::AppError;

/// A scan running on a background thread.
pub struct RunningScan {
    pub cancel: Arc<AtomicBool>,
}

/// Shared state managed by Tauri. Rust owns the index; the UI only asks.
pub struct AppState {
    db: Mutex<Db>,
    /// Background jobs open their own connection to this file (WAL mode).
    pub db_path: PathBuf,
    pub scan: Mutex<Option<RunningScan>>,
    pub last_progress: Mutex<Option<ScanProgress>>,
    /// Loaded offline geobase, if one is installed.
    pub geocoder: RwLock<Option<Arc<OfflineGeocoder>>>,
    /// Where an imported geobase is kept (app data directory).
    pub geobase_path: PathBuf,
    /// ExifTool / FFmpeg shipped with the installer, if present.
    pub helpers_dir: Option<PathBuf>,
    /// Thumbnail cache directory (app data).
    pub thumbs_dir: PathBuf,
    /// Built in the background at startup (probing helpers takes a moment).
    thumbnailer: RwLock<Option<Arc<Thumbnailer>>>,
}

/// Thumbnailer wired to the helpers that are available now.
pub fn build_thumbnailer(
    thumbs_dir: &std::path::Path,
    settings: &AppSettings,
    helpers_dir: Option<&std::path::Path>,
) -> Thumbnailer {
    let (helpers, statuses) = Helpers::detect(settings, helpers_dir);
    let exiftool = statuses
        .iter()
        .find(|s| s.tool == kadr_core::services::tools::Tool::Exiftool && s.version().is_some())
        .map(|s| PathBuf::from(&s.program));
    Thumbnailer::new(
        thumbs_dir.to_path_buf(),
        exiftool,
        helpers.ffmpeg,
        usize::from(settings.thumbnail_workers),
    )
}

impl AppState {
    pub fn new(
        db: Db,
        db_path: PathBuf,
        geobase_path: PathBuf,
        geocoder: Option<OfflineGeocoder>,
        helpers_dir: Option<PathBuf>,
        thumbs_dir: PathBuf,
    ) -> Self {
        Self {
            db: Mutex::new(db),
            db_path,
            scan: Mutex::new(None),
            last_progress: Mutex::new(None),
            geocoder: RwLock::new(geocoder.map(Arc::new)),
            geobase_path,
            helpers_dir,
            thumbs_dir,
            thumbnailer: RwLock::new(None),
        }
    }

    /// The thumbnailer, building it now if startup has not finished yet.
    pub fn thumbnailer(&self) -> Result<Arc<Thumbnailer>, AppError> {
        if let Some(t) = self.thumbnailer.read().ok().and_then(|t| t.clone()) {
            return Ok(t);
        }
        let settings = self.db()?.load_settings()?;
        Ok(self.rebuild_thumbnailer(&settings))
    }

    /// Re-detects helpers (startup, settings change) and installs a fresh
    /// thumbnailer.
    pub fn rebuild_thumbnailer(&self, settings: &AppSettings) -> Arc<Thumbnailer> {
        let fresh = Arc::new(build_thumbnailer(
            &self.thumbs_dir,
            settings,
            self.helpers_dir.as_deref(),
        ));
        if let Ok(mut slot) = self.thumbnailer.write() {
            *slot = Some(fresh.clone());
        }
        fresh
    }

    pub fn geocoder(&self) -> Option<Arc<OfflineGeocoder>> {
        self.geocoder.read().ok().and_then(|g| g.clone())
    }

    pub fn db(&self) -> Result<MutexGuard<'_, Db>, AppError> {
        self.db
            .lock()
            .map_err(|_| AppError::internal("index lock poisoned"))
    }

    pub fn lock<'a, T>(&self, m: &'a Mutex<T>) -> Result<MutexGuard<'a, T>, AppError> {
        m.lock()
            .map_err(|_| AppError::internal("state lock poisoned"))
    }
}
