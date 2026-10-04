use celesta_media::MediaError;
use celesta_remote::RemoteAssetError;
use std::error::Error;
use std::{fmt, io};

#[derive(Debug)]
pub enum RenderError {
    SurfaceTooLarge {
        width: u32,
        height: u32,
    },
    InvalidColor(String),
    UnsupportedRotation {
        layer: String,
        degrees: f64,
    },
    UnsupportedNonUniformTextScale {
        x: f64,
        y: f64,
    },
    RemoteAsset {
        asset: String,
        source: RemoteAssetError,
    },
    AssetIo {
        asset: String,
        source: io::Error,
    },
    ImageDecode {
        asset: String,
        source: image::ImageError,
    },
    PsdDecode {
        asset: String,
        source: psd::PsdError,
    },
    InvalidFont {
        asset: String,
        reason: &'static str,
    },
    MissingPsdLayer {
        asset: String,
        layer: String,
    },
    Media(MediaError),
    Time(celesta_composition::TimeError),
    Io(io::Error),
    Png(png::EncodingError),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SurfaceTooLarge { width, height } => {
                write!(formatter, "surface {width}x{height} is too large")
            }
            Self::InvalidColor(color) => write!(formatter, "invalid color `{color}`"),
            Self::UnsupportedRotation { layer, degrees } => write!(
                formatter,
                "CPU reference renderer does not support {degrees} degree rotation on layer `{layer}`"
            ),
            Self::UnsupportedNonUniformTextScale { x, y } => write!(
                formatter,
                "CPU reference renderer does not support non-uniform text scale ({x}, {y})"
            ),
            Self::RemoteAsset { asset, source } => {
                write!(formatter, "could not load remote asset `{asset}`: {source}")
            }
            Self::AssetIo { asset, source } => {
                write!(formatter, "could not read asset `{asset}`: {source}")
            }
            Self::ImageDecode { asset, source } => {
                write!(
                    formatter,
                    "could not decode image asset `{asset}`: {source}"
                )
            }
            Self::PsdDecode { asset, source } => {
                write!(formatter, "could not decode PSD asset `{asset}`: {source}")
            }
            Self::MissingPsdLayer { asset, layer } => {
                write!(formatter, "PSD asset `{asset}` has no layer `{layer}`")
            }
            Self::InvalidFont { asset, reason } => {
                write!(formatter, "could not load font `{asset}`: {reason}")
            }
            Self::Media(error) => write!(formatter, "could not decode video frame: {error}"),
            Self::Time(error) => write!(formatter, "could not calculate video time: {error}"),
            Self::Io(error) => write!(formatter, "image I/O failed: {error}"),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}

impl Error for RenderError {}

impl From<MediaError> for RenderError {
    fn from(error: MediaError) -> Self {
        Self::Media(error)
    }
}

impl From<celesta_composition::TimeError> for RenderError {
    fn from(error: celesta_composition::TimeError) -> Self {
        Self::Time(error)
    }
}
