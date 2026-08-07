//! `whirr` library crate: exposes the app state, sampler, and ui modules so
//! both the thin `main.rs` binary and the integration test suite
//! (`tests/render.rs`) can drive the same code paths.

pub mod units;

pub mod history;

pub mod mac;

pub mod app;
pub mod sampler;
pub mod ui;
pub mod host;
pub mod settings;
pub mod update;
