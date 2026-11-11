//! One test binary for every integration test that needs nothing but the
//! Rust build, so each links once. The tests whose server spawns the React
//! runtime are the `runtime` target (`tests/runtime/`).

#[path = "../common/mod.rs"]
mod common;
mod export;
mod render;
mod server;
