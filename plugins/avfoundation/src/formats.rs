//! The `CMFormatDescription`s the muxers give `AVAssetWriter` for each
//! track: the source format hint of its passthrough input, and the format of
//! every sample buffer appended to it.
//!
//! Video is described from its codec family, size, and the parameter sets
//! its encoder gave (`avcC`, `hvcC`; ProRes has none), tagged BT.709 limited
//! range, which `AVAssetWriter` writes as the `colr` box. AAC is described by
//! its AudioSpecificConfig, wrapped in the `ES_Descriptor` magic cookie
//! AudioToolbox itself gives; PCM by its packed little-endian layout.

use crate::color::check_color;
use crate::esds;
use celesta_media::{MediaError, Track, TrackConfig};
use celesta_plugin_host::codec_family;
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked,
    kAudioFormatFlagIsSignedInteger, kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
};
use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType,
};
use objc2_core_media::{
    CMAudioFormatDescription, CMAudioFormatDescriptionCreate, CMFormatDescription,
    CMVideoFormatDescription, CMVideoFormatDescriptionCreate,
    kCMFormatDescriptionColorPrimaries_ITU_R_709_2, kCMFormatDescriptionExtension_ColorPrimaries,
    kCMFormatDescriptionExtension_Depth, kCMFormatDescriptionExtension_FullRangeVideo,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms,
    kCMFormatDescriptionExtension_TransferFunction, kCMFormatDescriptionExtension_YCbCrMatrix,
    kCMFormatDescriptionTransferFunction_ITU_R_709_2, kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2,
};
use std::ffi::c_void;
use std::ptr::{NonNull, null};

/// The sample frames in each AAC-LC packet.
pub(crate) const AAC_FRAMES: i64 = 1024;

/// AAC-LC's codec string, the one AAC the muxers take: other object types
/// (HE-AAC's 2048-frame packets, say) are not described as they are.
pub(crate) const AAC_LC: &str = "mp4a.40.2";

/// A packed little-endian PCM layout: its codec string's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pcm {
    /// The bits of one sample.
    pub(crate) bits: u32,
    /// Whether samples are IEEE floats, else signed integers.
    pub(crate) float: bool,
}

impl Pcm {
    /// The layout `codec` names: `lpcm.s16le`, `lpcm.s24le`, or `lpcm.f32le`.
    pub(crate) fn from_codec(codec: &str) -> Option<Self> {
        let (bits, float) = match codec {
            "lpcm.s16le" => (16, false),
            "lpcm.s24le" => (24, false),
            "lpcm.f32le" => (32, true),
            _ => return None,
        };
        Some(Self { bits, float })
    }

    /// The bytes one sample frame of `channels` takes.
    pub(crate) fn frame_bytes(self, channels: u16) -> u32 {
        self.bits / 8 * u32::from(channels)
    }
}

/// The video codec type a codec family is stored as, and the extension
/// atom carrying its parameter sets, if any: `hev1` is stored as `hvc1`
/// (`codec_family` reads both alike), whose `hvcC` holds them.
fn video_codec(family: &str) -> Option<(&'static [u8; 4], Option<&'static str>)> {
    Some(match family {
        "avc1" => (b"avc1", Some("avcC")),
        "hvc1" => (b"hvc1", Some("hvcC")),
        "ap4h" => (b"ap4h", None),
        "apch" => (b"apch", None),
        _ => return None,
    })
}

/// The error for a track no sample description is made for.
fn undescribed(config: &TrackConfig) -> MediaError {
    MediaError::Codec(format!("no sample description for {}", config.codec))
}

/// `value` as an `i32`, for `what`.
pub(crate) fn int(value: u32, what: &str) -> Result<i32, MediaError> {
    i32::try_from(value).map_err(|_| MediaError::Codec(format!("a {what} of {value} is too large")))
}

/// A dictionary of `pairs`, as the untyped dictionary CoreMedia takes.
fn dictionary(pairs: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary> {
    let keys: Vec<&CFString> = pairs.iter().map(|(key, _)| *key).collect();
    let values: Vec<&CFType> = pairs.iter().map(|(_, value)| *value).collect();
    let typed = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: a dictionary is a dictionary whatever its keys and values.
    unsafe { CFRetained::cast_unchecked::<CFDictionary>(typed) }
}

/// The video format description of `track`: its codec type and size, its
/// parameter sets, and BT.709 limited range; for ProRes, its depth (32
/// with alpha, else 24).
pub(crate) fn video_format(
    track: &Track,
) -> Result<CFRetained<CMVideoFormatDescription>, MediaError> {
    let Track::Video {
        config,
        width,
        height,
        color,
        alpha,
        ..
    } = track
    else {
        return Err(MediaError::Codec(
            "an audio track has no video format".to_owned(),
        ));
    };
    let (codec_type, atom) =
        video_codec(&codec_family(&config.codec)).ok_or_else(|| undescribed(config))?;
    check_color(color)?;
    let (width, height) = (int(*width, "width")?, int(*height, "height")?);

    let record = match atom {
        Some(atom) => {
            let bytes = config.description.as_deref().ok_or_else(|| {
                MediaError::Codec(format!("a {} track has no {atom}", config.codec))
            })?;
            Some((CFString::from_str(atom), CFData::from_bytes(bytes)))
        }
        None => None,
    };
    let atoms = record
        .as_ref()
        .map(|(name, bytes)| dictionary(&[(name, bytes.as_ref())]));
    let full_range = CFBoolean::new(false);
    let depth = CFNumber::new_i32(if *alpha { 32 } else { 24 });
    // SAFETY (each static): an immutable CFString CoreMedia defines.
    let mut extensions: Vec<(&CFString, &CFType)> = unsafe {
        vec![
            (
                kCMFormatDescriptionExtension_ColorPrimaries,
                kCMFormatDescriptionColorPrimaries_ITU_R_709_2.as_ref(),
            ),
            (
                kCMFormatDescriptionExtension_TransferFunction,
                kCMFormatDescriptionTransferFunction_ITU_R_709_2.as_ref(),
            ),
            (
                kCMFormatDescriptionExtension_YCbCrMatrix,
                kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2.as_ref(),
            ),
            (
                kCMFormatDescriptionExtension_FullRangeVideo,
                full_range.as_ref(),
            ),
        ]
    };
    match &atoms {
        // SAFETY: an immutable CFString CoreMedia defines.
        Some(atoms) => extensions.push((
            unsafe { kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms },
            atoms.as_ref(),
        )),
        // ProRes.
        // SAFETY: as above.
        None => extensions.push((
            unsafe { kCMFormatDescriptionExtension_Depth },
            depth.as_ref(),
        )),
    }
    let extensions = dictionary(&extensions);

    let mut format: *const CMVideoFormatDescription = null();
    // SAFETY: the extensions are a dictionary of the documented keys and
    // value types; `format` is a valid place for the new description.
    let status = unsafe {
        CMVideoFormatDescriptionCreate(
            None,
            u32::from_be_bytes(*codec_type),
            width,
            height,
            Some(&extensions),
            NonNull::from(&mut format),
        )
    };
    created("CMVideoFormatDescriptionCreate", status, format)
}

/// The audio format description of `track`: AAC-LC (`mp4a.40.2`) with the
/// AudioToolbox magic cookie of its AudioSpecificConfig, or packed
/// little-endian PCM.
pub(crate) fn audio_format(
    track: &Track,
) -> Result<CFRetained<CMAudioFormatDescription>, MediaError> {
    let Track::Audio {
        config,
        sample_rate,
        channels,
        ..
    } = track
    else {
        return Err(MediaError::Codec(
            "a video track has no audio format".to_owned(),
        ));
    };
    let channels_per_frame = u32::from(*channels);
    let (description, cookie) = if config.codec == AAC_LC {
        let asc = config.description.as_deref().ok_or_else(|| {
            MediaError::Codec(format!(
                "a {} track has no AudioSpecificConfig",
                config.codec
            ))
        })?;
        let description = AudioStreamBasicDescription {
            mSampleRate: f64::from(*sample_rate),
            mFormatID: kAudioFormatMPEG4AAC,
            mFormatFlags: 0,
            mBytesPerPacket: 0,
            mFramesPerPacket: AAC_FRAMES as u32,
            mBytesPerFrame: 0,
            mChannelsPerFrame: channels_per_frame,
            mBitsPerChannel: 0,
            mReserved: 0,
        };
        (description, Some(esds::cookie_from_asc(asc, 0)))
    } else {
        let pcm = Pcm::from_codec(&config.codec).ok_or_else(|| undescribed(config))?;
        let kind = if pcm.float {
            kAudioFormatFlagIsFloat
        } else {
            kAudioFormatFlagIsSignedInteger
        };
        let frame_bytes = pcm.frame_bytes(*channels);
        let description = AudioStreamBasicDescription {
            mSampleRate: f64::from(*sample_rate),
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kind | kAudioFormatFlagIsPacked,
            mBytesPerPacket: frame_bytes,
            mFramesPerPacket: 1,
            mBytesPerFrame: frame_bytes,
            mChannelsPerFrame: channels_per_frame,
            mBitsPerChannel: pcm.bits,
            mReserved: 0,
        };
        (description, None)
    };
    let (cookie_size, cookie_bytes) = match &cookie {
        Some(cookie) => (cookie.len(), cookie.as_ptr().cast::<c_void>()),
        None => (0, null()),
    };
    let mut format: *const CMAudioFormatDescription = null();
    // SAFETY: `description` is a valid ASBD; the cookie, when there is one,
    // is `cookie_size` bytes (CoreMedia copies it); no channel layout or
    // extensions; `format` is a valid place for the new description.
    let status = unsafe {
        CMAudioFormatDescriptionCreate(
            None,
            NonNull::from(&description),
            0,
            null(),
            cookie_size,
            cookie_bytes,
            None,
            NonNull::from(&mut format),
        )
    };
    created("CMAudioFormatDescriptionCreate", status, format)
}

/// The description a `…FormatDescriptionCreate` call made, which the caller
/// owns, or why there is none.
fn created(
    call: &str,
    status: i32,
    format: *const CMFormatDescription,
) -> Result<CFRetained<CMFormatDescription>, MediaError> {
    if status != 0 {
        return Err(MediaError::Codec(format!("{call} failed: {status}")));
    }
    let format = NonNull::new(format.cast_mut())
        .ok_or_else(|| MediaError::Codec(format!("{call} gave no format description")))?;
    // SAFETY: the description is the caller's to release (the Create rule).
    Ok(unsafe { CFRetained::from_raw(format) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::Color;
    use objc2_core_audio_types::AudioStreamBasicDescription;
    use objc2_core_media::CMAudioFormatDescriptionGetStreamBasicDescription;

    fn config(codec: &str, description: Option<Vec<u8>>) -> TrackConfig {
        TrackConfig {
            codec: codec.to_owned(),
            description,
        }
    }

    fn video(config: TrackConfig, alpha: bool) -> Track {
        Track::Video {
            config,
            timescale: 30,
            width: 64,
            height: 36,
            frame_rate: Rational::new(30, 1),
            color: Color::bt709_limited(),
            alpha,
        }
    }

    fn audio(config: TrackConfig) -> Track {
        Track::Audio {
            config,
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 0,
        }
    }

    /// The four-character code of `format`'s media subtype.
    fn subtype(format: &CMFormatDescription) -> [u8; 4] {
        // SAFETY: a valid format description.
        unsafe { format.media_sub_type() }.to_be_bytes()
    }

    #[test]
    fn video_is_stored_by_its_family() {
        for (codec, stored) in [
            ("ap4h", *b"ap4h"),
            ("apch", *b"apch"),
            ("avc1.640028", *b"avc1"),
            ("avc3.640028", *b"avc1"),
            ("hev1.1.6.L93.B0", *b"hvc1"),
        ] {
            let format = video_format(&video(config(codec, Some(vec![1])), false)).unwrap();
            assert_eq!(subtype(&format), stored, "{codec}");
            // SAFETY: a valid format description.
            assert_eq!(unsafe { format.media_type() }, u32::from_be_bytes(*b"vide"));
        }
        assert!(matches!(
            video_format(&video(config("avc1.640028", None), false)),
            Err(MediaError::Codec(message)) if message == "a avc1.640028 track has no avcC"
        ));
        assert!(video_format(&video(config("vp09.00.10.08", None), false)).is_err());
        let mut unknown = video(config("ap4h", None), true);
        if let Track::Video { color, .. } = &mut unknown {
            color.transfer = "smpte2084".into();
        }
        assert!(matches!(
            video_format(&unknown),
            Err(MediaError::Codec(message)) if message == "unknown color transfer `smpte2084`"
        ));
        assert!(video_format(&audio(config("mp4a.40.2", Some(vec![0x11, 0x90])))).is_err());
    }

    fn basic(format: &CMFormatDescription) -> AudioStreamBasicDescription {
        // SAFETY: an audio format description, whose ASBD lives as long as it.
        unsafe { *CMAudioFormatDescriptionGetStreamBasicDescription(format) }
    }

    #[test]
    fn audio_is_aac_or_packed_pcm() {
        let aac = audio_format(&audio(config("mp4a.40.2", Some(vec![0x11, 0x90])))).unwrap();
        let described = basic(&aac);
        assert_eq!(
            (
                described.mFormatID,
                described.mFramesPerPacket,
                described.mChannelsPerFrame,
                described.mSampleRate
            ),
            (kAudioFormatMPEG4AAC, 1024, 2, 48_000.0)
        );
        assert!(audio_format(&audio(config("mp4a.40.2", None))).is_err());
        // Not AAC-LC: HE-AAC's packets are 2048 frames.
        assert!(matches!(
            audio_format(&audio(config("mp4a.40.5", Some(vec![0x2b, 0x92, 0x08, 0x00])))),
            Err(MediaError::Codec(message)) if message == "no sample description for mp4a.40.5"
        ));
        assert!(audio_format(&audio(config("mp4a", Some(vec![0x11, 0x90])))).is_err());
        for (codec, bits, flags) in [
            ("lpcm.s16le", 16, kAudioFormatFlagIsSignedInteger),
            ("lpcm.s24le", 24, kAudioFormatFlagIsSignedInteger),
            ("lpcm.f32le", 32, kAudioFormatFlagIsFloat),
        ] {
            let described = basic(&audio_format(&audio(config(codec, None))).unwrap());
            assert_eq!(
                (
                    described.mFormatID,
                    described.mBitsPerChannel,
                    described.mBytesPerFrame,
                    described.mFormatFlags
                ),
                (
                    kAudioFormatLinearPCM,
                    bits,
                    bits / 8 * 2,
                    flags | kAudioFormatFlagIsPacked
                ),
                "{codec}"
            );
        }
        assert!(audio_format(&audio(config("opus", None))).is_err());
        assert!(audio_format(&video(config("ap4h", None), false)).is_err());
    }
}
