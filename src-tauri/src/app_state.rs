use std::sync::{Mutex, MutexGuard};

use kadr_core::persistence::Db;

use crate::errors::AppError;

/// Shared state managed by Tauri. Rust owns the index; the UI only asks.
pub struct AppState {
    db: Mutex<Db>,
}

impl AppState {
    pub fn new(db: Db) -> Self {
        Self { db: Mutex::new(db) }
    }

    pub fn db(&self) -> Result<MutexGuard<'_, Db>, AppError> {
        self.db.lock().map_err(|_| AppError::internal("index lock poisoned"))
    }
}
