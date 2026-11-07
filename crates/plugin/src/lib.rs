//! Celesta content plugins: the 2D meshes or 3D scenes a plugin returns,
//! the provider interface the renderer asks through, and the protocol a
//! plugin process speaks. See `docs/handoff/2026-11-01-plugin-system.md`.
//!
//! This crate depends on nothing of Celesta's (only serde, serde_json,
//! thiserror, and libc), so a plugin can use it without linking any part of
//! Celesta.

mod mesh;
pub mod protocol;
mod provider;
pub mod scene;
pub mod shared;

pub use mesh::{MeshBlend, MeshFrame, MeshInfo, MeshMask, MeshModel, MeshState, TextureSource};
pub use protocol::{
    MEDIA_PROTOCOL_VERSION, PROTOCOL_VERSION, protocol_for_roles, serve, serve_requests,
};
pub use provider::{
    ContentFrame, ContentModel, ContentProvider, ModelId, Notice, NoticeLevel, Opened, PluginError,
};
pub use scene::{
    AlphaMode, Camera, ColorSpace, CullFace, CustomMaterial, DepthBias, DirectionalLight, DrawInfo,
    DrawState, EmptyTexel, Filter, Geometry, Lights, Material, MaterialKind, MaterialOverrides,
    MorphTarget, OutlinePass, Projection, Sampler, SceneFrame, SceneModel, SceneShader,
    SceneTexture, ShaderTexture, Skin, TextureRef, TextureTransform, UnlitMaterial, Wrap,
};
