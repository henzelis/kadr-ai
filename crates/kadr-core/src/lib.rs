//! Kadr core: the domain model, settings and the local SQLite index.
//!
//! Nothing here depends on Tauri or the UI, so every rule can be unit-tested
//! on its own and reused by a CLI or another shell later.

pub mod domain;
pub mod errors;
pub mod persistence;
pub mod services;
pub mod settings;

pub use errors::{CoreError, CoreResult};
