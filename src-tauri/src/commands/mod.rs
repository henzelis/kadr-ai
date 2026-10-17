//! Typed Tauri commands. The UI never passes shell commands or raw file
//! operations; every command validates its input in Rust.

pub mod library;
pub mod settings;
