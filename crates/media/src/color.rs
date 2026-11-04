//! Video color tags, and the conversion of HDR or wide-gamut video to the
//! 8-bit sRGB frames the renderers composite.
//!
//! The conversion reproduces what Chrome does when a page draws such a video
//! onto an SDR canvas, so a composition looks the same in `@celesta.studio/web`:
//! Skia's Reference White Tone Mapping Operator (RWTMO, SMPTE ST 2094-50
//! Annex C) at a targeted HDR headroom of 0, applied to maxRGB in linear
//! BT.2020 light relative to an HDR reference white of 203 nits, then a
//! gamut conversion to BT.709 that clips what falls outside it.

use rayon::prelude::*;

/// A video stream's color tags, by FFmpeg's names for them (`bt2020`,
/// `smpte2084`, `bt2020nc`, …). An untagged property is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// Reads FFmpeg's raw `AVColorPrimaries`, `AVColorTransferCharacteristic`,
    /// and `AVColorSpace` values.
    pub(crate) fn from_raw(primaries: i32, transfer: i32, matrix: i32) -> Self {
        Self {
            primaries: primaries_name(primaries).map(str::to_owned),
            transfer: transfer_name(transfer).map(str::to_owned),
            matrix: matrix_name(matrix).map(str::to_owned),
        }
    }

    pub fn dynamic_range(&self) -> DynamicRange {
        match self.transfer.as_deref() {
            Some("smpte2084") => DynamicRange::Pq,
            Some("arib-std-b67") => DynamicRange::Hlg,
            _ => DynamicRange::Sdr,
        }
    }

    /// How frames of this stream must be converted, or `None` for a stream
    /// the packed RGBA path already shows correctly. That path converts
    /// YUV to RGB with the tagged matrix but leaves the transfer and the
    /// primaries alone, so it serves BT.709-like video only — and FFmpeg's
    /// frame export refuses every stream tagged BT.2020, PQ, or HLG.
    pub(crate) fn conversion(&self) -> Option<ColorConversion> {
        let bt2020 = self.primaries.as_deref() == Some("bt2020");
        let bt2020_matrix = matches!(self.matrix.as_deref(), Some("bt2020nc" | "bt2020c"));
        let transfer = match self.dynamic_range() {
            DynamicRange::Pq => Transfer::Pq,
            DynamicRange::Hlg => Transfer::Hlg,
            // Chrome decodes BT.709, BT.601, and BT.2020 SDR video with the
            // sRGB curve.
            DynamicRange::Sdr if bt2020 || bt2020_matrix => Transfer::Srgb,
            DynamicRange::Sdr => return None,
        };
        Some(ColorConversion { transfer, bt2020 })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transfer {
    Srgb,
    Pq,
    Hlg,
}

/// The conversion a stream's frames need: its transfer characteristic,
/// and whether its primaries are BT.2020 rather than BT.709.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColorConversion {
    pub(crate) transfer: Transfer,
    pub(crate) bt2020: bool,
}

/// The luminance, in nits, that HDR video maps to SDR white (ITU-R BT.2408).
const HDR_REFERENCE_WHITE: f32 = 203.0;

/// The display peak HLG is decoded for, and the content peak assumed when
/// the stream carries no light-level metadata (as Chrome does).
pub(crate) const DEFAULT_PEAK_LUMINANCE: f32 = 1000.0;

/// Skia's largest HDR headroom, in stops.
const MAX_HDR_HEADROOM: f32 = 6.0;

/// Linear BT.2020 to linear BT.709, both D65.
const BT2020_TO_BT709: [[f32; 3]; 3] = [
    [1.660_491, -0.587_641_1, -0.072_849_86],
    [-0.124_550_5, 1.132_899_9, -0.008_349_42],
    [-0.018_150_76, -0.100_578_9, 1.118_729_6],
];

const BT2020_LUMINANCE: [f32; 3] = [0.2627, 0.6780, 0.0593];
const BT709_LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The samples in the tone curve's lookup table between the knee and the
/// content peak.
const GAIN_SAMPLES: usize = 16_384;

/// Converts 16-bit nonlinear R'G'B' frames to 8-bit sRGB RGBA.
pub(crate) struct ToneMapper {
    /// The top `DECODE_BITS` of a code value to linear light: relative to
    /// the HDR reference white for PQ; HLG's inverse OETF (0–1), before its
    /// OOTF; sRGB's EOTF.
    decode: Vec<f32>,
    ootf: Option<Ootf>,
    curve: Option<GainCurve>,
    gamut: Option<[[f32; 3]; 3]>,
    /// Linear light at `ENCODE_STEPS` resolution to an sRGB code value.
    encode: Vec<u8>,
}

/// HLG's OOTF for a 1000-nit display, scaled so the HDR reference white is
/// 1.0: `E * Y^(γ-1) * 1000 / 203`.
struct Ootf {
    luminance: [f32; 3],
    /// `Y^(γ-1) * 1000 / 203` indexed by the top bits of `Y`'s float
    /// representation, a logarithmic spacing that is finest where the curve
    /// is steepest.
    scales: Vec<f32>,
}

const SYSTEM_GAMMA: f32 = 1.2;

/// The smallest luminance the OOTF table resolves; anything darker
/// encodes to black.
const OOTF_MIN_LUMINANCE: f32 = 1.0 / (1 << 24) as f32;

/// The float mantissa bits dropped from an OOTF table index.
const OOTF_SHIFT: u32 = 13;

impl Ootf {
    fn new(luminance: [f32; 3]) -> Self {
        let first = OOTF_MIN_LUMINANCE.to_bits() >> OOTF_SHIFT;
        let last = 1.0_f32.to_bits() >> OOTF_SHIFT;
        let scales = (first..=last)
            .map(|index| {
                // The middle of the luminances that share this entry.
                let y = f32::from_bits((index << OOTF_SHIFT) | (1 << (OOTF_SHIFT - 1)));
                y.powf(SYSTEM_GAMMA - 1.0) * DEFAULT_PEAK_LUMINANCE / HDR_REFERENCE_WHITE
            })
            .collect();
        Self { luminance, scales }
    }

    fn scale(&self, rgb: [f32; 3]) -> f32 {
        let y = dot(self.luminance, rgb).clamp(OOTF_MIN_LUMINANCE, 1.0);
        let first = OOTF_MIN_LUMINANCE.to_bits() >> OOTF_SHIFT;
        self.scales[((y.to_bits() >> OOTF_SHIFT) - first) as usize]
    }
}

const ENCODE_STEPS: usize = 65_535;

/// The decode table's index bits: 12 keep every code of 10-bit video apart,
/// and its 16 KiB stay in L1.
const DECODE_BITS: u32 = 12;

impl ToneMapper {
    /// `peak_luminance` is the content's peak in nits: its MaxCLL, else its
    /// mastering display's peak, else [`DEFAULT_PEAK_LUMINANCE`].
    pub(crate) fn new(conversion: ColorConversion, peak_luminance: f32) -> Self {
        let decode = (0..1 << DECODE_BITS)
            .map(|code| {
                // The center of the 16-bit codes that share this entry.
                let signal = (code as f32 + 0.5) / (1 << DECODE_BITS) as f32;
                match conversion.transfer {
                    Transfer::Srgb => srgb_eotf(signal),
                    Transfer::Pq => pq_eotf(signal) / HDR_REFERENCE_WHITE,
                    Transfer::Hlg => hlg_inverse_oetf(signal),
                }
            })
            .collect();
        let ootf = (conversion.transfer == Transfer::Hlg).then(|| {
            Ootf::new(if conversion.bt2020 {
                BT2020_LUMINANCE
            } else {
                BT709_LUMINANCE
            })
        });
        let curve = match conversion.transfer {
            Transfer::Srgb => None,
            Transfer::Pq | Transfer::Hlg => GainCurve::rwtmo(peak_luminance),
        };
        let encode = (0..=ENCODE_STEPS)
            .map(|step| {
                let linear = step as f32 / ENCODE_STEPS as f32;
                (srgb_oetf(linear) * 255.0).round() as u8
            })
            .collect();
        Self {
            decode,
            ootf,
            curve,
            gamut: conversion.bt2020.then_some(BT2020_TO_BT709),
            encode,
        }
    }

    /// Converts tightly packed little-endian RGB48 rows into RGBA8.
    pub(crate) fn convert(&self, rgb48: &[u8], width: usize, rgba: &mut [u8]) {
        let row_in = width * 6;
        let row_out = width * 4;
        rgba.par_chunks_mut(row_out)
            .zip(rgb48.par_chunks(row_in))
            .for_each(|(out, input)| {
                for (pixel, source) in out
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(input.as_chunks::<6>().0)
                {
                    let code = |index: usize| {
                        usize::from(u16::from_le_bytes([source[index], source[index + 1]]))
                            >> (16 - DECODE_BITS)
                    };
                    let rgb = self.map([
                        self.decode[code(0)],
                        self.decode[code(2)],
                        self.decode[code(4)],
                    ]);
                    *pixel = [
                        self.encode_channel(rgb[0]),
                        self.encode_channel(rgb[1]),
                        self.encode_channel(rgb[2]),
                        u8::MAX,
                    ];
                }
            });
    }

    /// Linear source light to linear BT.709 light, before clipping.
    fn map(&self, mut rgb: [f32; 3]) -> [f32; 3] {
        if let Some(ootf) = &self.ootf {
            let scale = ootf.scale(rgb);
            rgb = rgb.map(|channel| channel * scale);
        }
        if let Some(curve) = &self.curve {
            let gain = curve.gain(rgb[0].max(rgb[1]).max(rgb[2]));
            rgb = rgb.map(|channel| channel * gain);
        }
        match &self.gamut {
            Some(matrix) => matrix.map(|row| dot(row, rgb)),
            None => rgb,
        }
    }

    fn encode_channel(&self, linear: f32) -> u8 {
        self.encode[(linear.clamp(0.0, 1.0) * ENCODE_STEPS as f32 + 0.5) as usize]
    }
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

/// The RWTMO gain for the SDR alternate image, as a linear multiplier of
/// maxRGB `x`: `y_white` up to the reference white, a Bézier shoulder up to
/// the content peak, and a hard clip to 1.0 past it.
struct GainCurve {
    knee_gain: f32,
    peak: f32,
    /// Linear gains at `GAIN_SAMPLES + 1` evenly spaced points of `[1, peak]`.
    samples: Vec<f32>,
    /// Samples per unit of `x`.
    step: f32,
}

impl GainCurve {
    /// `None` when the content peaks at or below the reference white, where
    /// Skia leaves the colors alone.
    fn rwtmo(peak_luminance: f32) -> Option<Self> {
        if peak_luminance.is_nan() || peak_luminance <= HDR_REFERENCE_WHITE {
            return None;
        }
        let headroom = (peak_luminance / HDR_REFERENCE_WHITE)
            .log2()
            .min(MAX_HDR_HEADROOM);
        let points = rwtmo_control_points(headroom);
        let peak = headroom.exp2();
        let samples = (0..=GAIN_SAMPLES)
            .map(|index| {
                let x = 1.0 + (peak - 1.0) * index as f32 / GAIN_SAMPLES as f32;
                evaluate_gain_curve(&points, x).exp2()
            })
            .collect();
        Some(Self {
            knee_gain: points[0].y.exp2(),
            peak,
            samples,
            step: GAIN_SAMPLES as f32 / (peak - 1.0),
        })
    }

    fn gain(&self, x: f32) -> f32 {
        if x <= 1.0 {
            return self.knee_gain;
        }
        if x >= self.peak {
            return 1.0 / x;
        }
        self.samples[((x - 1.0) * self.step + 0.5) as usize]
    }
}

/// A gain curve control point: `x` in linear light, `y` the log2 gain, and
/// `m` its slope.
#[derive(Clone, Copy, Debug)]
struct ControlPoint {
    x: f32,
    y: f32,
    m: f32,
}

/// Skia's `PopulateUsingRwtmo` for the alternate image at headroom 0
/// (SMPTE ST 2094-50 Formulas C.2–C.6).
fn rwtmo_control_points(headroom: f32) -> [ControlPoint; 8] {
    let ratio = (headroom / (1000.0_f32 / HDR_REFERENCE_WHITE).log2()).min(1.0);
    let y_white = 1.0 - 0.5 * ratio;
    let kappa = 0.65_f32;
    let (x_knee, y_knee) = (1.0_f32, y_white);
    let (x_max, y_max) = (headroom.exp2(), 1.0_f32);
    let x_mid = (1.0 - kappa) * x_knee + kappa * (x_knee * y_max / y_knee);
    let y_mid = (1.0 - kappa) * y_knee + kappa * y_max;
    let (x_a, y_a) = (x_knee - 2.0 * x_mid + x_max, y_knee - 2.0 * y_mid + y_max);
    let (x_b, y_b) = (2.0 * x_mid - 2.0 * x_knee, 2.0 * y_mid - 2.0 * y_knee);
    std::array::from_fn(|index| {
        let t = index as f32 / 7.0;
        let x = x_knee + t * (x_b + t * x_a);
        let y = y_knee + t * (y_b + t * y_a);
        let m = (2.0 * y_a * t + y_b) / (2.0 * x_a * t + x_b);
        ControlPoint {
            x,
            y: (y / x).log2(),
            m: (x * m - y) / (std::f32::consts::LN_2 * x * y),
        }
    })
}

/// SMPTE ST 2094-50 Formula (11): the cubic Hermite interpolation of the
/// control points, extended flat on the left and as a clip on the right.
fn evaluate_gain_curve(points: &[ControlPoint], x: f32) -> f32 {
    let first = points[0];
    let last = points[points.len() - 1];
    if x <= first.x {
        return first.y;
    }
    if x >= last.x {
        return last.y + (last.x / x).log2();
    }
    let index = points.partition_point(|point| point.x <= x) - 1;
    let (left, right) = (points[index], points[index + 1]);
    let h = right.x - left.x;
    if h == 0.0 {
        return left.y;
    }
    let (m_left, m_right) = (left.m * h, right.m * h);
    let c3 = 2.0 * left.y + m_left - 2.0 * right.y + m_right;
    let c2 = -3.0 * left.y + 3.0 * right.y - 2.0 * m_left - m_right;
    let t = (x - left.x) / h;
    ((c3 * t + c2) * t + m_left) * t + left.y
}

/// SMPTE ST 2084's EOTF: a signal (0–1) to nits.
fn pq_eotf(signal: f32) -> f32 {
    const M1: f64 = 2610.0 / 16384.0;
    const M2: f64 = 2523.0 / 4096.0 * 128.0;
    const C1: f64 = 3424.0 / 4096.0;
    const C2: f64 = 2413.0 / 4096.0 * 32.0;
    const C3: f64 = 2392.0 / 4096.0 * 32.0;
    let p = f64::from(signal.max(0.0)).powf(1.0 / M2);
    (10_000.0 * ((p - C1).max(0.0) / (C2 - C3 * p)).powf(1.0 / M1)) as f32
}

/// ARIB STD-B67's inverse OETF: a signal (0–1) to scene light (0–1).
fn hlg_inverse_oetf(signal: f32) -> f32 {
    const A: f32 = 0.178_832_77;
    const B: f32 = 0.284_668_92;
    const C: f32 = 0.559_910_7;
    if signal <= 0.5 {
        signal * signal / 3.0
    } else {
        (((signal - C) / A).exp() + B) / 12.0
    }
}

fn srgb_eotf(signal: f32) -> f32 {
    if signal <= 0.040_45 {
        signal / 12.92
    } else {
        ((signal + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_oetf(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// FFmpeg's `av_color_primaries_name`.
fn primaries_name(value: i32) -> Option<&'static str> {
    Some(match value {
        1 => "bt709",
        4 => "bt470m",
        5 => "bt470bg",
        6 => "smpte170m",
        7 => "smpte240m",
        8 => "film",
        9 => "bt2020",
        10 => "smpte428",
        11 => "smpte431",
        12 => "smpte432",
        22 => "ebu3213",
        _ => return None,
    })
}

/// FFmpeg's `av_color_transfer_name`.
fn transfer_name(value: i32) -> Option<&'static str> {
    Some(match value {
        1 => "bt709",
        4 => "gamma22",
        5 => "gamma28",
        6 => "smpte170m",
        7 => "smpte240m",
        8 => "linear",
        9 => "log100",
        10 => "log316",
        11 => "iec61966-2-4",
        12 => "bt1361e",
        13 => "iec61966-2-1",
        14 => "bt2020-10",
        15 => "bt2020-12",
        16 => "smpte2084",
        17 => "smpte428",
        18 => "arib-std-b67",
        _ => return None,
    })
}

/// FFmpeg's `av_color_space_name`.
fn matrix_name(value: i32) -> Option<&'static str> {
    Some(match value {
        0 => "gbr",
        1 => "bt709",
        4 => "fcc",
        5 => "bt470bg",
        6 => "smpte170m",
        7 => "smpte240m",
        8 => "ycgco",
        9 => "bt2020nc",
        10 => "bt2020c",
        11 => "smpte2085",
        12 => "chroma-derived-nc",
        13 => "chroma-derived-c",
        14 => "ictcp",
        15 => "ipt-c2",
        16 => "ycgco-re",
        17 => "ycgco-ro",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PQ: ColorConversion = ColorConversion {
        transfer: Transfer::Pq,
        bt2020: true,
    };
    const HLG: ColorConversion = ColorConversion {
        transfer: Transfer::Hlg,
        bt2020: true,
    };

    /// SMPTE ST 2084's inverse EOTF: nits to a signal (0–1).
    fn pq_signal(nits: f64) -> f64 {
        let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
        let (c1, c2, c3) = (
            3424.0 / 4096.0,
            2413.0 / 4096.0 * 32.0,
            2392.0 / 4096.0 * 32.0,
        );
        let p = (nits / 10_000.0).powf(m1);
        ((c1 + c2 * p) / (1.0 + c3 * p)).powf(m2)
    }

    fn convert(conversion: ColorConversion, peak: f32, signal: [f64; 3]) -> [u8; 3] {
        let rgb48: Vec<u8> = signal
            .iter()
            .flat_map(|value| ((value * 65_535.0).round() as u16).to_le_bytes())
            .collect();
        let mut rgba = [0; 4];
        ToneMapper::new(conversion, peak).convert(&rgb48, 1, &mut rgba);
        assert_eq!(rgba[3], u8::MAX);
        [rgba[0], rgba[1], rgba[2]]
    }

    fn gray(conversion: ColorConversion, peak: f32, signal: f64) -> u8 {
        let [r, g, b] = convert(conversion, peak, [signal; 3]);
        assert!(r == g && g == b, "gray stays gray: {r} {g} {b}");
        r
    }

    fn assert_near(actual: u8, expected: u8) {
        assert_within(actual, expected, 1);
    }

    fn assert_within(actual: u8, expected: u8, tolerance: u8) {
        assert!(
            actual.abs_diff(expected) <= tolerance,
            "got {actual}, Chrome draws {expected}"
        );
    }

    #[test]
    fn classifies_streams_by_their_tags() {
        let color = |primaries: &str, transfer: &str, matrix: &str| VideoColor {
            primaries: Some(primaries.to_owned()),
            transfer: Some(transfer.to_owned()),
            matrix: Some(matrix.to_owned()),
        };
        assert_eq!(VideoColor::default().conversion(), None);
        assert_eq!(color("bt709", "bt709", "bt709").conversion(), None);
        assert_eq!(
            color("bt709", "bt709", "bt709").dynamic_range(),
            DynamicRange::Sdr
        );
        let pq = color("bt2020", "smpte2084", "bt2020nc");
        assert_eq!(pq.dynamic_range(), DynamicRange::Pq);
        assert_eq!(pq.conversion(), Some(PQ));
        let hlg = color("bt2020", "arib-std-b67", "bt2020nc");
        assert_eq!(hlg.dynamic_range(), DynamicRange::Hlg);
        assert_eq!(hlg.conversion(), Some(HLG));
        assert_eq!(
            color("bt2020", "bt2020-10", "bt2020nc").conversion(),
            Some(ColorConversion {
                transfer: Transfer::Srgb,
                bt2020: true,
            })
        );
        assert_eq!(
            VideoColor::from_raw(9, 18, 9),
            color("bt2020", "arib-std-b67", "bt2020nc")
        );
        assert_eq!(VideoColor::from_raw(2, 2, 2), VideoColor::default());
    }

    /// Chrome 152 drew these PQ grays (nits) onto an sRGB canvas from a
    /// lossless 10-bit HEVC clip without light-level metadata.
    #[test]
    fn tone_maps_pq_like_chrome() {
        for (nits, chrome) in [
            (0.0, 0),
            (1.0, 8),
            (10.0, 44),
            (25.0, 70),
            (100.0, 137),
            (203.0, 188),
            (300.0, 212),
            (500.0, 233),
            (1000.0, 255),
            (4000.0, 255),
        ] {
            assert_near(gray(PQ, DEFAULT_PEAK_LUMINANCE, pq_signal(nits)), chrome);
        }
        // The reference white lands at half of SDR white's light.
        assert_eq!(gray(PQ, DEFAULT_PEAK_LUMINANCE, pq_signal(203.0)), 188);
    }

    /// The same clip with MaxCLL 4000: the shoulder reaches further, and
    /// everything up to the reference white is unchanged.
    #[test]
    fn follows_the_content_peak() {
        for (nits, chrome) in [(100.0, 137), (300.0, 201), (1000.0, 226), (2000.0, 241)] {
            assert_near(gray(PQ, 4000.0, pq_signal(nits)), chrome);
        }
        // Content that peaks at the reference white is not tone mapped.
        assert_eq!(gray(PQ, 203.0, pq_signal(100.0)), 186);
    }

    /// Chrome's HLG grays, at signal `n / 15`.
    #[test]
    fn tone_maps_hlg_like_chrome() {
        for (step, chrome) in [
            (0, 0),
            (2, 16),
            (5, 61),
            (8, 107),
            (10, 150),
            (12, 208),
            (14, 242),
            (15, 255),
        ] {
            assert_near(
                gray(HLG, DEFAULT_PEAK_LUMINANCE, f64::from(step) / 15.0),
                chrome,
            );
        }
        // HLG's 75% signal is the reference white.
        assert_near(gray(HLG, DEFAULT_PEAK_LUMINANCE, 0.75), 188);
    }

    /// BT.2020 primaries convert to BT.709, clipping what falls outside.
    #[test]
    fn clips_wide_gamut_primaries_to_bt709() {
        let red = pq_signal(100.0);
        let [r, g, b] = convert(PQ, DEFAULT_PEAK_LUMINANCE, [red, 0.0, 0.0]);
        // Chrome's half-float pipeline lands a little higher on saturated
        // colors.
        assert_within(r, 173, 2);
        assert_eq!((g, b), (0, 0));
        let [r, g, b] = convert(HLG, DEFAULT_PEAK_LUMINANCE, [0.0, 7.0 / 15.0, 0.0]);
        assert_eq!((r, b), (0, 0));
        assert_within(g, 94, 2);
        // SDR BT.2020 decodes with the sRGB curve, without tone mapping.
        let srgb = ColorConversion {
            transfer: Transfer::Srgb,
            bt2020: true,
        };
        assert_eq!(gray(srgb, DEFAULT_PEAK_LUMINANCE, 0.5), 128);
    }

    #[test]
    fn converts_every_row_of_a_frame() {
        let (width, height) = (5, 3);
        let rgb48: Vec<u8> = (0..width * height)
            .flat_map(|pixel| [pixel as u16 * 4096; 3])
            .flat_map(u16::to_le_bytes)
            .collect();
        let mut rgba = vec![0; width * height * 4];
        ToneMapper::new(HLG, DEFAULT_PEAK_LUMINANCE).convert(&rgb48, width, &mut rgba);
        let reds: Vec<u8> = rgba.chunks(4).map(|pixel| pixel[0]).collect();
        assert!(reds.windows(2).all(|pair| pair[0] < pair[1]), "{reds:?}");
        assert!(rgba.chunks(4).all(|pixel| pixel[3] == u8::MAX));
    }
}
