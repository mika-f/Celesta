use crate::error::ExportError;
use crate::{AUDIO_SECONDS_TIMESCALE, DEFAULT_REACT_AUDIO_SAMPLE_RATE};
use celesta_composition::{
    AssetLocation, AudioClip, AudioGraph, Layer, LayerContent, ResolvedAsset, Time, TimeRange,
};
use celesta_evaluator::Evaluator;
use celesta_project::{Project, TimelineContent};
use celesta_react_bridge::ReactAudioClipDescriptor;
use std::path::Path;

/// Drops `audio` timeline items before evaluating a companion project's
/// *visual* content for a React export. `Evaluator::visual_layer` already
/// evaluates an `audio` item to no layer (`TimelineContent::Audio { .. } =>
/// return Ok(None)`), so this filter is a cheap, explicit skip rather than a
/// behavior change; an `audio` item has nothing to contribute to
/// `<ProjectTimeline />`/`<ProjectTrack />` either way. This filtering is
/// specific to the visual path: the companion project's *audio* mixdown
/// (`build_audio_graph` below) deliberately uses the project unfiltered, via
/// `Evaluator::audio_graph()`, so its audio timeline items do play. Every
/// other visual kind reaches
/// `<ProjectTimeline />`/`<ProjectTrack />`: `component` items evaluate to
/// `LayerContent::MissingComponent`, which `@celesta/react` resolves against
/// its own `registerComponent()` registry (falling back to leaving
/// `missingComponent` layers as-is, which `GpuRenderer` then errors on);
/// `dialogue` items evaluate to a `LayerContent::Group` of the character's
/// portrait image and subtitle text (`Evaluator::dialogue`) — there is no
/// dedicated `LayerContent::Dialogue`, so it needs no special handling here
/// or on the TypeScript side, the same generic `Group`/`Image`/`Text`
/// rendering every other layer already gets.
pub(crate) fn visual_only_project(project: &Project) -> Project {
    let mut filtered = project.clone();
    for track in &mut filtered.tracks {
        track
            .items
            .retain(|item| !matches!(item.content, TimelineContent::Audio { .. }));
    }
    filtered
}

/// Builds the complete `AudioGraph` a React export mixes down: every
/// `<Audio>` clip the entry's frames reported (`react_audio`, accumulated by
/// `render_react_video` — one raw entry per clip per frame) plus, when a
/// companion project is given, that project's own complete, *unfiltered*
/// `Evaluator::audio_graph()` (unlike the visual path's
/// `visual_only_project()`, which drops `TimelineContent::Audio` items
/// because they contribute nothing visually — the audio mixdown needs them
/// to actually play). The companion project supplies the graph's
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

    // The same declaration reports once per rendered frame; merge identical
    // reports (bit-identical because every field is recomputed from the same
    // inputs each frame) while preserving multiplicity — two distinct clips
    // with identical parameters at the same instant really do play twice.
    let merged = merge_react_audio_clips(react_audio);

    for (index, (clip, _)) in merged.iter().enumerate() {
        let mut asset = ResolvedAsset {
            id: clip.src.clone(),
            location: AssetLocation::File {
                path: clip.src.clone(),
            },
        };
        absolutize_asset(&mut asset, react_asset_root);
        graph.clips.push(AudioClip {
            id: format!("react-audio:{index}"),
            asset,
            range: TimeRange {
                start: seconds_to_time(clip.start),
                duration: seconds_to_time(clip.duration),
            },
            source_start: seconds_to_time(clip.source_start),
            source_duration: None,
            playback_rate: clip.playback_rate.clone(),
            volume: clip.volume.clone(),
            muted: clip.muted,
        });
    }

    Ok(graph)
}

/// Collapses per-frame audio reports into one entry per distinct clip,
/// keeping first-seen order and how many frames reported it (its
/// multiplicity — two identical clips playing at once must stay two clips).
pub(crate) fn merge_react_audio_clips(
    clips: &[ReactAudioClipDescriptor],
) -> Vec<(ReactAudioClipDescriptor, usize)> {
    let mut merged: Vec<(ReactAudioClipDescriptor, usize)> = Vec::new();
    for clip in clips {
        match merged.iter_mut().find(|(known, _)| known == clip) {
            Some((_, count)) => *count += 1,
            None => merged.push((clip.clone(), 1)),
        }
    }
    merged
}

pub(crate) fn seconds_to_time(seconds: f64) -> Time {
    Time::new(
        (seconds * f64::from(AUDIO_SECONDS_TIMESCALE)).round() as i64,
        AUDIO_SECONDS_TIMESCALE,
    )
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
        LayerContent::Group { layers, .. } => absolutize_layers(layers, asset_root),
        LayerContent::Text { .. }
        | LayerContent::Rect { .. }
        | LayerContent::Path { .. }
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
