use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use kadr_core::persistence::Db;
use kadr_core::services::geocoder::OfflineGeocoder;
use kadr_core::services::indexer::ScanProgress;

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
}

impl AppState {
    pub fn new(
        db: Db,
        db_path: PathBuf,
        geobase_path: PathBuf,
        geocoder: Option<OfflineGeocoder>,
        helpers_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            db: Mutex::new(db),
            db_path,
            scan: Mutex::new(None),
            last_progress: Mutex::new(None),
            geocoder: RwLock::new(geocoder.map(Arc::new)),
            geobase_path,
            helpers_dir,
        }
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
