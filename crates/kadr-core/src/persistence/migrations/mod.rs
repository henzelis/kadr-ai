//! Schema migrations, applied in order and tracked with `PRAGMA user_version`.
//! Migrations are append-only: never edit one that has shipped.

pub(crate) const MIGRATIONS: &[&str] = &[include_str!("0001_init.sql")];
