use crate::DEFAULT_REACT_AUDIO_SAMPLE_RATE;
use crate::error::ExportError;
use celesta_composition::{
    AssetLocation, AudioGraph, InstanceShape, Layer, LayerContent, ResolvedAsset,
};
use celesta_evaluator::Evaluator;
use celesta_project::Project;
use celesta_react_bridge::{ReactAudioClipDescriptor, react_audio_clips};
use std::path::Path;

/// Builds the complete `AudioGraph` a React export mixes down: every
/// `<Audio>` clip the entry's frames reported (`react_audio`, accumulated by
/// `render_react_video` — one raw entry per clip per frame) plus, when a
/// companion project is given, that project's own complete
/// `Evaluator::audio_graph()`. The companion project supplies the graph's
/// `sample_rate`/`master_volume` when present, since it already carries
/// authoritative values for those; otherwise `DEFAULT_REACT_AUDIO_SAMPLE_RATE`
/// is used.
pub(crate) fn build_audio_graph(
    react_audio: &[ReactAudioClipDescriptor],
    react_asset_root: &Path,
    project: Option<(&Project, &Path)>,
) -> Result<AudioGraph, ExportError> {
    let mut graph = match project {
        Some((project, project_asset_root)) => {
            let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
            let mut graph = evaluator.audio_graph().map_err(ExportError::Evaluation)?;
            for clip in &mut graph.clips {
                absolutize_asset(&mut clip.asset, project_asset_root);
            }
            graph
        }
        None => AudioGraph {
            sample_rate: DEFAULT_REACT_AUDIO_SAMPLE_RATE,
            master_volume: 1.0,
            clips: Vec::new(),
        },
    };

    // The bridge builds the clips the same way for the preview: identical
    // per-frame reports collapse into one clip, a remote `src` stays a URL,
    // and a relative one resolves against the entry's own directory.
    graph
        .clips
        .extend(react_audio_clips(react_audio, react_asset_root).map_err(ExportError::React)?);

    Ok(graph)
}

/// Rewrites a project-evaluated layer tree's relative asset paths into
/// absolute ones. A React export's `GpuRenderer` has a single `asset_root`
/// (the entry's own directory); a companion project's assets may live
/// elsewhere, so its evaluated layers carry absolute paths instead of
/// relying on that shared root.
pub(crate) fn absolutize_layers(layers: &mut [Layer], asset_root: &Path) {
    for layer in layers {
        absolutize_layer_content(&mut layer.content, asset_root);
    }
}

pub(crate) fn absolutize_layer_content(content: &mut LayerContent, asset_root: &Path) {
    match content {
        LayerContent::Video { asset, .. }
        | LayerContent::Image { asset, .. }
        | LayerContent::Psd { asset, .. } => {
            absolutize_asset(asset, asset_root);
        }
        LayerContent::Plugin { assets, .. } => {
            for asset in assets.values_mut() {
                absolutize_asset(asset, asset_root);
            }
        }
        LayerContent::Group {
            layers,
            clip: _,
            mask,
            motion_blur: _,
        } => {
            absolutize_layers(layers, asset_root);
            if let Some(mask) = mask {
                absolutize_layers(&mut mask.layers, asset_root);
            }
        }
        LayerContent::Instances { shape, .. } => {
            if let InstanceShape::Image { asset } = shape.as_mut() {
                absolutize_asset(asset, asset_root);
            }
        }
        LayerContent::Text { .. }
        | LayerContent::Rect { .. }
        | LayerContent::Path { .. }
        | LayerContent::StaticRef { .. }
        | LayerContent::MissingComponent { .. } => {}
    }
}

pub(crate) fn absolutize_fonts(fonts: &mut [ResolvedAsset], asset_root: &Path) {
    for font in fonts {
        absolutize_asset(font, asset_root);
    }
}

pub(crate) fn absolutize_asset(asset: &mut ResolvedAsset, asset_root: &Path) {
    if let AssetLocation::File { path } = &mut asset.location {
        let candidate = Path::new(path.as_str());
        if candidate.is_relative() {
            *path = asset_root.join(candidate).to_string_lossy().into_owned();
        }
    }
}
