//! A [`Color`]'s names as libavutil's color enums, which tag encoders,
//! frames, and streams, and as the coefficients libswscale converts RGB
//! with.

use celesta_media::{Color, MediaError};
use ffmpeg_sys_next::{
    AVCodecContext, AVCodecParameters, AVColorPrimaries, AVColorRange, AVColorSpace,
    AVColorTransferCharacteristic, AVFrame, SWS_CS_BT2020, SWS_CS_ITU601, SWS_CS_ITU709,
    av_color_primaries_from_name, av_color_primaries_name, av_color_space_from_name,
    av_color_space_name, av_color_transfer_from_name, av_color_transfer_name, sws_getCoefficients,
};
use std::ffi::{CStr, CString, c_char, c_int};

/// A [`Color`] in libavutil's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AvColor {
    pub(crate) primaries: AVColorPrimaries,
    pub(crate) transfer: AVColorTransferCharacteristic,
    pub(crate) matrix: AVColorSpace,
    pub(crate) range: AVColorRange,
}

/// The refusal of `name` for the color field `field`.
fn unknown(field: &str, name: &str) -> MediaError {
    MediaError::Codec(format!("unknown color {field} `{name}`"))
}

/// The enumerator of a bindgen enum whose value libavutil returned, or none
/// for one the bindings lack (a libavutil newer than their headers), so no
/// integer is ever reinterpreted as an enum it may not be.
macro_rules! enumerator {
    ($function:ident, $enum:ident { $($variant:ident),* $(,)? }) => {
        fn $function(value: c_int) -> Option<$enum> {
            [$($enum::$variant),*]
                .into_iter()
                .find(|known| *known as c_int == value)
        }
    };
}

enumerator!(
    primaries,
    AVColorPrimaries {
        AVCOL_PRI_RESERVED0,
        AVCOL_PRI_BT709,
        AVCOL_PRI_UNSPECIFIED,
        AVCOL_PRI_RESERVED,
        AVCOL_PRI_BT470M,
        AVCOL_PRI_BT470BG,
        AVCOL_PRI_SMPTE170M,
        AVCOL_PRI_SMPTE240M,
        AVCOL_PRI_FILM,
        AVCOL_PRI_BT2020,
        AVCOL_PRI_SMPTE428,
        AVCOL_PRI_SMPTE431,
        AVCOL_PRI_SMPTE432,
        AVCOL_PRI_EBU3213,
        AVCOL_PRI_EXT_BASE,
    }
);

enumerator!(
    transfer,
    AVColorTransferCharacteristic {
        AVCOL_TRC_RESERVED0,
        AVCOL_TRC_BT709,
        AVCOL_TRC_UNSPECIFIED,
        AVCOL_TRC_RESERVED,
        AVCOL_TRC_GAMMA22,
        AVCOL_TRC_GAMMA28,
        AVCOL_TRC_SMPTE170M,
        AVCOL_TRC_SMPTE240M,
        AVCOL_TRC_LINEAR,
        AVCOL_TRC_LOG,
        AVCOL_TRC_LOG_SQRT,
        AVCOL_TRC_IEC61966_2_4,
        AVCOL_TRC_BT1361_ECG,
        AVCOL_TRC_IEC61966_2_1,
        AVCOL_TRC_BT2020_10,
        AVCOL_TRC_BT2020_12,
        AVCOL_TRC_SMPTE2084,
        AVCOL_TRC_SMPTE428,
        AVCOL_TRC_ARIB_STD_B67,
        AVCOL_TRC_EXT_BASE,
    }
);

enumerator!(
    matrix,
    AVColorSpace {
        AVCOL_SPC_RGB,
        AVCOL_SPC_BT709,
        AVCOL_SPC_UNSPECIFIED,
        AVCOL_SPC_RESERVED,
        AVCOL_SPC_FCC,
        AVCOL_SPC_BT470BG,
        AVCOL_SPC_SMPTE170M,
        AVCOL_SPC_SMPTE240M,
        AVCOL_SPC_YCGCO,
        AVCOL_SPC_BT2020_NCL,
        AVCOL_SPC_BT2020_CL,
        AVCOL_SPC_SMPTE2085,
        AVCOL_SPC_CHROMA_DERIVED_NCL,
        AVCOL_SPC_CHROMA_DERIVED_CL,
        AVCOL_SPC_ICTCP,
        AVCOL_SPC_IPT_C2,
        AVCOL_SPC_YCGCO_RE,
        AVCOL_SPC_YCGCO_RO,
    }
);

/// The value libavutil names `name`, through its `from_name` lookup, the
/// bindings' enumerator of the integer it returns, and back through
/// `to_name`. FFmpeg 8's lookup is exact already; the round trip is a
/// defensive check that a name is taken only when it is libavutil's own
/// spelling exactly, whatever lookup the linked libavutil has.
fn lookup<T: Copy>(
    field: &str,
    name: &str,
    from_name: unsafe extern "C" fn(*const c_char) -> c_int,
    enumerator: fn(c_int) -> Option<T>,
    to_name: unsafe extern "C" fn(T) -> *const c_char,
) -> Result<T, MediaError> {
    let text = CString::new(name).map_err(|_| unknown(field, name))?;
    // SAFETY: `text` is NUL-terminated and outlives the call.
    let value = unsafe { from_name(text.as_ptr()) };
    let value = enumerator(value).ok_or_else(|| unknown(field, name))?;
    // SAFETY: `value` is an enumerator of the enum `to_name` takes.
    let canonical = unsafe { to_name(value) };
    // SAFETY: libavutil's names are static NUL-terminated strings, or null.
    if canonical.is_null() || unsafe { CStr::from_ptr(canonical) } != text.as_c_str() {
        return Err(unknown(field, name));
    }
    Ok(value)
}

impl AvColor {
    /// `color`'s names as libavutil knows them; a name it does not know is
    /// refused, naming the field, rather than left unspecified.
    pub(crate) fn from_color(color: &Color) -> Result<Self, MediaError> {
        let primaries = lookup(
            "primaries",
            &color.primaries,
            av_color_primaries_from_name,
            primaries,
            av_color_primaries_name,
        )?;
        let transfer = lookup(
            "transfer",
            &color.transfer,
            av_color_transfer_from_name,
            transfer,
            av_color_transfer_name,
        )?;
        let matrix = lookup(
            "matrix",
            &color.matrix,
            av_color_space_from_name,
            matrix,
            av_color_space_name,
        )?;
        let range = match color.range.as_str() {
            "limited" | "tv" | "mpeg" => AVColorRange::AVCOL_RANGE_MPEG,
            "full" | "pc" | "jpeg" => AVColorRange::AVCOL_RANGE_JPEG,
            other => return Err(unknown("range", other)),
        };
        Ok(Self {
            primaries,
            transfer,
            matrix,
            range,
        })
    }

    /// Tags an encoder, before `avcodec_open2`, which writes the tags into
    /// the bitstream where the codec has room (H.264's VUI, ProRes's frame
    /// header, VP9's color space).
    ///
    /// # Safety
    ///
    /// `context` is a writable `AVCodecContext`.
    pub(crate) unsafe fn apply_to_codec(&self, context: *mut AVCodecContext) {
        // SAFETY: the caller's context is writable.
        unsafe {
            (*context).colorspace = self.matrix;
            (*context).color_primaries = self.primaries;
            (*context).color_trc = self.transfer;
            (*context).color_range = self.range;
        }
    }

    /// Tags a frame sent to an encoder.
    ///
    /// # Safety
    ///
    /// `frame` is a writable `AVFrame`.
    pub(crate) unsafe fn apply_to_frame(&self, frame: *mut AVFrame) {
        // SAFETY: the caller's frame is writable.
        unsafe {
            (*frame).colorspace = self.matrix;
            (*frame).color_primaries = self.primaries;
            (*frame).color_trc = self.transfer;
            (*frame).color_range = self.range;
        }
    }

    /// Tags a stream's codec parameters.
    ///
    /// # Safety
    ///
    /// `parameters` is a stream's writable `AVCodecParameters`.
    pub(crate) unsafe fn apply_to_codecpar(&self, parameters: *mut AVCodecParameters) {
        // SAFETY: the caller's parameters are writable.
        unsafe {
            (*parameters).color_space = self.matrix;
            (*parameters).color_primaries = self.primaries;
            (*parameters).color_trc = self.transfer;
            (*parameters).color_range = self.range;
        }
    }

    /// libswscale's coefficient table for the matrix and the destination
    /// range `sws_setColorspaceDetails` takes (1 for full, 0 for limited).
    /// A matrix libswscale has no table for is refused by its name.
    pub(crate) fn sws_coefficients(&self) -> Result<(*const c_int, c_int), MediaError> {
        let table = match self.matrix {
            AVColorSpace::AVCOL_SPC_BT709 => SWS_CS_ITU709,
            AVColorSpace::AVCOL_SPC_BT470BG | AVColorSpace::AVCOL_SPC_SMPTE170M => SWS_CS_ITU601,
            AVColorSpace::AVCOL_SPC_BT2020_NCL => SWS_CS_BT2020,
            other => {
                // SAFETY: returns one of libavutil's static names, or null.
                let name = unsafe { av_color_space_name(other) };
                // The name `from_color` was given, which round-trips exactly.
                let name = if name.is_null() {
                    format!("{other:?}")
                } else {
                    // SAFETY: a static NUL-terminated string.
                    unsafe { CStr::from_ptr(name) }
                        .to_string_lossy()
                        .into_owned()
                };
                return Err(MediaError::Codec(format!(
                    "libswscale cannot convert RGB to the `{name}` matrix"
                )));
            }
        };
        let range = c_int::from(self.range == AVColorRange::AVCOL_RANGE_JPEG);
        // SAFETY: returns one of libswscale's static tables.
        Ok((unsafe { sws_getCoefficients(table) }, range))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(primaries: &str, transfer: &str, matrix: &str, range: &str) -> Color {
        Color {
            primaries: primaries.to_owned(),
            transfer: transfer.to_owned(),
            matrix: matrix.to_owned(),
            range: range.to_owned(),
        }
    }

    #[test]
    fn bt709_limited_is_libavutils_bt709_in_mpeg_range() {
        assert_eq!(
            AvColor::from_color(&Color::bt709_limited()).unwrap(),
            AvColor {
                primaries: AVColorPrimaries::AVCOL_PRI_BT709,
                transfer: AVColorTransferCharacteristic::AVCOL_TRC_BT709,
                matrix: AVColorSpace::AVCOL_SPC_BT709,
                range: AVColorRange::AVCOL_RANGE_MPEG,
            }
        );
        let other = AvColor::from_color(&color("bt2020", "smpte2084", "bt2020nc", "pc")).unwrap();
        assert_eq!(
            other,
            AvColor {
                primaries: AVColorPrimaries::AVCOL_PRI_BT2020,
                transfer: AVColorTransferCharacteristic::AVCOL_TRC_SMPTE2084,
                matrix: AVColorSpace::AVCOL_SPC_BT2020_NCL,
                range: AVColorRange::AVCOL_RANGE_JPEG,
            }
        );
        for (name, range) in [
            ("tv", AVColorRange::AVCOL_RANGE_MPEG),
            ("mpeg", AVColorRange::AVCOL_RANGE_MPEG),
            ("full", AVColorRange::AVCOL_RANGE_JPEG),
            ("jpeg", AVColorRange::AVCOL_RANGE_JPEG),
        ] {
            let parsed = AvColor::from_color(&color("bt709", "bt709", "bt709", name)).unwrap();
            assert_eq!(parsed.range, range, "{name}");
        }
    }

    #[test]
    fn a_name_libavutil_does_not_know_is_refused_by_field() {
        for (given, message) in [
            (
                color("p3", "bt709", "bt709", "limited"),
                "unknown color primaries `p3`",
            ),
            (
                color("bt709", "srgb", "bt709", "limited"),
                "unknown color transfer `srgb`",
            ),
            // Starts with libavutil's `bt709` but is not its name.
            (
                color("bt709", "bt709", "bt709x", "limited"),
                "unknown color matrix `bt709x`",
            ),
            (
                color("bt709", "bt709", "bt709", ""),
                "unknown color range ``",
            ),
            (
                color("bt\x00709", "bt709", "bt709", "limited"),
                "unknown color primaries `bt\x00709`",
            ),
        ] {
            let Err(MediaError::Codec(refusal)) = AvColor::from_color(&given) else {
                panic!("{given:?} was taken");
            };
            assert_eq!(refusal, message);
        }
    }

    #[test]
    fn libswscale_takes_the_matrixs_table_and_the_range() {
        // SAFETY: libswscale's static tables.
        let table = |id| unsafe { sws_getCoefficients(id) };
        let limited = AvColor::from_color(&Color::bt709_limited()).unwrap();
        assert_eq!(
            limited.sws_coefficients().unwrap(),
            (table(SWS_CS_ITU709), 0)
        );
        let full = AvColor::from_color(&color("bt709", "bt709", "smpte170m", "full")).unwrap();
        assert_eq!(full.sws_coefficients().unwrap(), (table(SWS_CS_ITU601), 1));
        let ycgco = AvColor::from_color(&color("bt709", "bt709", "ycgco", "limited")).unwrap();
        let Err(MediaError::Codec(refusal)) = ycgco.sws_coefficients() else {
            panic!("libswscale took ycgco");
        };
        assert_eq!(
            refusal,
            "libswscale cannot convert RGB to the `ycgco` matrix"
        );
    }

    #[test]
    fn every_target_gets_all_four_tags() {
        let color = AvColor::from_color(&Color::bt709_limited()).unwrap();
        // SAFETY: freshly allocated objects of each kind, freed after.
        unsafe {
            let mut context = ffmpeg_sys_next::avcodec_alloc_context3(std::ptr::null());
            color.apply_to_codec(context);
            assert_eq!(
                (
                    (*context).colorspace,
                    (*context).color_primaries,
                    (*context).color_trc,
                    (*context).color_range
                ),
                (color.matrix, color.primaries, color.transfer, color.range)
            );
            ffmpeg_sys_next::avcodec_free_context(&mut context);
            let mut frame = ffmpeg_sys_next::av_frame_alloc();
            color.apply_to_frame(frame);
            assert_eq!(
                (
                    (*frame).colorspace,
                    (*frame).color_primaries,
                    (*frame).color_trc,
                    (*frame).color_range
                ),
                (color.matrix, color.primaries, color.transfer, color.range)
            );
            ffmpeg_sys_next::av_frame_free(&mut frame);
            let mut parameters = ffmpeg_sys_next::avcodec_parameters_alloc();
            color.apply_to_codecpar(parameters);
            assert_eq!(
                (
                    (*parameters).color_space,
                    (*parameters).color_primaries,
                    (*parameters).color_trc,
                    (*parameters).color_range
                ),
                (color.matrix, color.primaries, color.transfer, color.range)
            );
            ffmpeg_sys_next::avcodec_parameters_free(&mut parameters);
        }
    }
}
