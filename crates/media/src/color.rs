//! Video color tags. Converting HDR or wide-gamut video to the 8-bit sRGB
//! frames the renderers composite is the codec plugin's work.

use serde::{Deserialize, Serialize};

/// A video stream's color tags, by FFmpeg's names for them (`bt2020`,
/// `smpte2084`, `bt2020nc`, …). An untagged property is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoColor {
    pub primaries: Option<String>,
    pub transfer: Option<String>,
    pub matrix: Option<String>,
}

/// The dynamic range a stream's transfer characteristic declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicRange {
    Sdr,
    /// SMPTE ST 2084 (HDR10, Dolby Vision profile 8.1).
    Pq,
    /// ARIB STD-B67 (iPhone HDR, Dolby Vision profile 8.4).
    Hlg,
}

impl DynamicRange {
    pub const fn is_hdr(self) -> bool {
        !matches!(self, Self::Sdr)
    }
}

impl VideoColor {
    pub fn dynamic_range(&self) -> DynamicRange {
        match self.transfer.as_deref() {
            Some("smpte2084") => DynamicRange::Pq,
            Some("arib-std-b67") => DynamicRange::Hlg,
            _ => DynamicRange::Sdr,
        }
    }
}
