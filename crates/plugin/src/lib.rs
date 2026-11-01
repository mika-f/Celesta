//! Celesta content plugins: the meshes a plugin returns, the provider
//! interface the renderer asks through, and the protocol a plugin process
//! speaks. See `docs/handoff/2026-11-01-plugin-system.md`.
//!
//! This crate depends on nothing of Celesta's (only serde, and libc on Unix),
//! so a plugin can use it without linking any part of Celesta.

mod mesh;
pub mod protocol;
mod provider;

pub use mesh::{MeshBlend, MeshFrame, MeshInfo, MeshMask, MeshModel, MeshState, TextureSource};
pub use protocol::{PROTOCOL_VERSION, serve};
pub use provider::{ContentProvider, ModelId, PluginError};
