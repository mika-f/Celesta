use celesta_media::MediaError;
use celesta_remote::RemoteAssetError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("surface {width}x{height} is too large")]
    SurfaceTooLarge { width: u32, height: u32 },
    #[error("invalid color `{0}`")]
    InvalidColor(String),
    #[error(
        "text color run {index} ({start}..{end}) is invalid for {text_length} code points; runs must be ordered, non-overlapping ranges within the text"
    )]
    InvalidTextColorRun {
        index: usize,
        start: usize,
        end: usize,
        text_length: usize,
    },
    #[error(
        "text font run {index} ({start}..{end}) is invalid for {text_length} code points; runs must be ordered, non-overlapping ranges within the text"
    )]
    InvalidTextFontRun {
        index: usize,
        start: usize,
        end: usize,
        text_length: usize,
    },
    #[error("CPU reference renderer does not support {degrees} degree rotation on layer `{layer}`")]
    UnsupportedRotation { layer: String, degrees: f64 },
    #[error("CPU reference renderer does not support `{property}` on layer `{layer}`")]
    UnsupportedTransform {
        layer: String,
        property: &'static str,
    },
    #[error("CPU reference renderer does not support non-uniform text scale ({x}, {y})")]
    UnsupportedNonUniformTextScale { x: f64, y: f64 },
    #[error(
        "CPU reference renderer does not support filters (custom shaders and color filters) on layer `{layer}`"
    )]
    UnsupportedShader { layer: String },
    /// A `LayerContent::StaticRef` reached the renderer; the React bridge
    /// expands every one before it hands a scene on.
    #[error("layer `{layer}` refers to a static subtree that was never expanded")]
    UnresolvedStatic { layer: String },
    #[error("could not load remote asset `{asset}`: {source}")]
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    #[error("could not read asset `{asset}`: {source}")]
    AssetIo { asset: String, source: io::Error },
    #[error("could not decode image asset `{asset}`: {source}")]
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    #[error("could not decode PSD asset `{asset}`: {source}")]
    PsdDecode {
        asset: String,
        source: psd::PsdError,
    },
    #[error("could not load font `{asset}`: {reason}")]
    InvalidFont { asset: String, reason: &'static str },
    #[error("PSD asset `{asset}` has no layer `{layer}`")]
    MissingPsdLayer { asset: String, layer: String },
    #[error("could not decode video frame: {0}")]
    Media(#[from] MediaError),
    #[error("could not calculate video time: {0}")]
    Time(#[from] celesta_composition::TimeError),
    #[error("image I/O failed: {0}")]
    Io(#[source] io::Error),
    #[error("PNG encoding failed: {0}")]
    Png(#[source] png::EncodingError),
}
