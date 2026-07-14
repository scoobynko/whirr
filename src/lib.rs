//! `whirr` library crate: exposes the app state, sampler, and ui modules so
//! both the thin `main.rs` binary and the integration test suite
//! (`tests/render.rs`) can drive the same code paths.

#[allow(dead_code)]
pub mod units;

#[allow(dead_code)]
pub mod history;

#[allow(dead_code)]
pub mod mac;

pub mod app;
pub mod sampler;
pub mod ui;
