use crate::error::RenderError;
use crate::psd_source;
use crate::types::RgbaFrame;
use celesta_composition::ResolvedAsset;
use celesta_remote::resolve_asset_path;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn read_asset(asset: &str, path: &Path) -> Result<Vec<u8>, RenderError> {
    fs::read(path).map_err(|source| RenderError::AssetIo {
        asset: asset.to_owned(),
        source,
    })
}

/// TrueType, OpenType, collection, WOFF, or WOFF2 magic.
pub(crate) fn is_sfnt_or_woff(data: &[u8]) -> bool {
    matches!(
        data.get(..4),
        Some(b"\0\x01\0\0" | b"OTTO" | b"true" | b"ttcf" | b"wOFF" | b"wOF2")
    )
}

pub(crate) fn local_asset_path(
    asset_root: &Path,
    asset: &ResolvedAsset,
) -> Result<PathBuf, RenderError> {
    resolve_asset_path(asset_root, &asset.location).map_err(|source| RenderError::RemoteAsset {
        asset: asset.id.clone(),
        source,
    })
}

/// Rasterizes a PSD portrait into a full-canvas RGBA frame, with layer
/// visibility resolved as [`psd_source::PsdSources::render`] describes.
pub fn rasterize_psd(
    asset: &str,
    path: &Path,
    visible_layers: &[String],
    enabled_layers: &[String],
    disabled_layers: &[String],
) -> Result<RgbaFrame, RenderError> {
    let image = psd_source::PsdSources::default().render(
        asset,
        path,
        visible_layers,
        enabled_layers,
        disabled_layers,
        1.0,
    )?;
    Ok(RgbaFrame {
        width: image.width,
        height: image.height,
        pixels: Arc::unwrap_or_clone(image.pixels),
    })
}
