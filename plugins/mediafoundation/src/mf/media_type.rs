//! The `IMFMediaType`s the encoders are given: the video output and input
//! types (the Decisions' H.264 and HEVC), the AAC and PCM audio types, and
//! the BT.709 limited colour every video type carries.

use crate::components::{Profile, VideoCodec};
use crate::mf::mft::check;
use celesta_composition::Rational;
use celesta_media::{MediaError, PixelFormat};
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, MF_MT_AAC_PAYLOAD_TYPE, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO,
    MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES,
    MF_MT_YUV_MATRIX, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateMediaType, MFMediaType_Audio,
    MFMediaType_Video, MFNominalRange_16_235, MFVideoFormat_H264, MFVideoFormat_HEVC,
    MFVideoFormat_NV12, MFVideoFormat_P010, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709,
    MFVideoTransFunc_709, MFVideoTransferMatrix_BT709, eAVEncH264VProfile_Base,
    eAVEncH264VProfile_High, eAVEncH264VProfile_Main, eAVEncH265VProfile_Main_420_8,
    eAVEncH265VProfile_Main_420_10,
};
use windows_core::GUID;

/// The two halves of a packed `UINT64` attribute (frame size, rate, aspect
/// ratio): `high` in the upper 32 bits.
fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// A new media type of `major` and `subtype`.
fn media_type(major: GUID, subtype: GUID) -> Result<IMFMediaType, MediaError> {
    // SAFETY: plain COM calls on a live interface.
    unsafe {
        let made = check("MFCreateMediaType", MFCreateMediaType())?;
        check(
            "SetGUID(MF_MT_MAJOR_TYPE)",
            made.SetGUID(&MF_MT_MAJOR_TYPE, &major),
        )?;
        check(
            "SetGUID(MF_MT_SUBTYPE)",
            made.SetGUID(&MF_MT_SUBTYPE, &subtype),
        )?;
        Ok(made)
    }
}

/// `value` as `media_type`'s `UINT32` attribute `key`, named `name`.
fn set_u32(
    media_type: &IMFMediaType,
    name: &str,
    key: &GUID,
    value: u32,
) -> Result<(), MediaError> {
    // SAFETY: a plain COM call on a live interface.
    check(&format!("SetUINT32({name})"), unsafe {
        media_type.SetUINT32(key, value)
    })
}

/// `value` as `media_type`'s `UINT64` attribute `key`, named `name`.
fn set_u64(
    media_type: &IMFMediaType,
    name: &str,
    key: &GUID,
    value: u64,
) -> Result<(), MediaError> {
    // SAFETY: a plain COM call on a live interface.
    check(&format!("SetUINT64({name})"), unsafe {
        media_type.SetUINT64(key, value)
    })
}

/// Tags `media_type` BT.709 primaries, transfer, and matrix, limited range.
pub(crate) fn set_color(media_type: &IMFMediaType) -> Result<(), MediaError> {
    set_u32(
        media_type,
        "MF_MT_VIDEO_PRIMARIES",
        &MF_MT_VIDEO_PRIMARIES,
        MFVideoPrimaries_BT709.0 as u32,
    )?;
    set_u32(
        media_type,
        "MF_MT_TRANSFER_FUNCTION",
        &MF_MT_TRANSFER_FUNCTION,
        MFVideoTransFunc_709.0 as u32,
    )?;
    set_u32(
        media_type,
        "MF_MT_YUV_MATRIX",
        &MF_MT_YUV_MATRIX,
        MFVideoTransferMatrix_BT709.0 as u32,
    )?;
    set_u32(
        media_type,
        "MF_MT_VIDEO_NOMINAL_RANGE",
        &MF_MT_VIDEO_NOMINAL_RANGE,
        MFNominalRange_16_235.0 as u32,
    )
}

/// A progressive video type of `subtype` at `width` x `height` and `rate`
/// frames a second, with square pixels and BT.709 limited colour.
fn video_type(
    subtype: GUID,
    width: u32,
    height: u32,
    rate: Rational,
) -> Result<IMFMediaType, MediaError> {
    let made = media_type(MFMediaType_Video, subtype)?;
    set_u64(
        &made,
        "MF_MT_FRAME_SIZE",
        &MF_MT_FRAME_SIZE,
        pack(width, height),
    )?;
    set_u64(
        &made,
        "MF_MT_FRAME_RATE",
        &MF_MT_FRAME_RATE,
        pack(rate.numerator, rate.denominator),
    )?;
    set_u64(
        &made,
        "MF_MT_PIXEL_ASPECT_RATIO",
        &MF_MT_PIXEL_ASPECT_RATIO,
        pack(1, 1),
    )?;
    set_u32(
        &made,
        "MF_MT_INTERLACE_MODE",
        &MF_MT_INTERLACE_MODE,
        MFVideoInterlace_Progressive.0 as u32,
    )?;
    set_color(&made)?;
    Ok(made)
}

/// `profile`'s `MF_MT_MPEG2_PROFILE` value.
pub(crate) fn profile_value(profile: Profile) -> u32 {
    (match profile {
        Profile::H264High => eAVEncH264VProfile_High.0,
        Profile::H264Main => eAVEncH264VProfile_Main.0,
        Profile::H264Baseline => eAVEncH264VProfile_Base.0,
        Profile::HevcMain => eAVEncH265VProfile_Main_420_8.0,
        Profile::HevcMain10 => eAVEncH265VProfile_Main_420_10.0,
    }) as u32
}

/// The output type of `codec`'s encoder: its subtype at `width` x `height`
/// and `rate`, `bitrate` (bit/s) on average, in `profile`.
pub(crate) fn encoder_output(
    codec: VideoCodec,
    profile: Profile,
    width: u32,
    height: u32,
    rate: Rational,
    bitrate: u32,
) -> Result<IMFMediaType, MediaError> {
    let subtype = match codec {
        VideoCodec::H264 => MFVideoFormat_H264,
        VideoCodec::Hevc => MFVideoFormat_HEVC,
    };
    let made = video_type(subtype, width, height, rate)?;
    set_u32(&made, "MF_MT_AVG_BITRATE", &MF_MT_AVG_BITRATE, bitrate)?;
    set_u32(
        &made,
        "MF_MT_MPEG2_PROFILE",
        &MF_MT_MPEG2_PROFILE,
        profile_value(profile),
    )?;
    Ok(made)
}

/// The subtype an encoder takes frames of `input` in: NV12 for 8 bits, P010
/// for 10 (RGBA is converted to it first).
pub(crate) fn input_subtype(input: PixelFormat) -> GUID {
    match input {
        PixelFormat::P010 | PixelFormat::Rgba16 | PixelFormat::Yuv420p10 => MFVideoFormat_P010,
        PixelFormat::Nv12 | PixelFormat::Rgba8 | PixelFormat::Yuv420p => MFVideoFormat_NV12,
    }
}

/// The input type of an encoder given frames of `input` at `width` x
/// `height` and `rate`.
pub(crate) fn encoder_input(
    input: PixelFormat,
    width: u32,
    height: u32,
    rate: Rational,
) -> Result<IMFMediaType, MediaError> {
    video_type(input_subtype(input), width, height, rate)
}

/// The AAC encoder's output type: raw AAC (no ADTS) of 16-bit `channels`
/// at `sample_rate`, `bitrate` bit/s.
pub(crate) fn aac_output(
    sample_rate: u32,
    channels: u32,
    bitrate: u32,
) -> Result<IMFMediaType, MediaError> {
    let made = media_type(MFMediaType_Audio, MFAudioFormat_AAC)?;
    set_u32(&made, "MF_MT_AAC_PAYLOAD_TYPE", &MF_MT_AAC_PAYLOAD_TYPE, 0)?;
    set_u32(
        &made,
        "MF_MT_AUDIO_SAMPLES_PER_SECOND",
        &MF_MT_AUDIO_SAMPLES_PER_SECOND,
        sample_rate,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_NUM_CHANNELS",
        &MF_MT_AUDIO_NUM_CHANNELS,
        channels,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_BITS_PER_SAMPLE",
        &MF_MT_AUDIO_BITS_PER_SAMPLE,
        16,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_AVG_BYTES_PER_SECOND",
        &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        bitrate / 8,
    )?;
    Ok(made)
}

/// The AAC encoder's input type: interleaved 16-bit PCM of `channels` at
/// `sample_rate`.
pub(crate) fn pcm_input(sample_rate: u32, channels: u32) -> Result<IMFMediaType, MediaError> {
    let made = media_type(MFMediaType_Audio, MFAudioFormat_PCM)?;
    set_u32(
        &made,
        "MF_MT_AUDIO_SAMPLES_PER_SECOND",
        &MF_MT_AUDIO_SAMPLES_PER_SECOND,
        sample_rate,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_NUM_CHANNELS",
        &MF_MT_AUDIO_NUM_CHANNELS,
        channels,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_BITS_PER_SAMPLE",
        &MF_MT_AUDIO_BITS_PER_SAMPLE,
        16,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_BLOCK_ALIGNMENT",
        &MF_MT_AUDIO_BLOCK_ALIGNMENT,
        channels * 2,
    )?;
    set_u32(
        &made,
        "MF_MT_AUDIO_AVG_BYTES_PER_SECOND",
        &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        sample_rate * channels * 2,
    )?;
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mf::startup::Started;

    fn u32_of(media_type: &IMFMediaType, key: &GUID) -> u32 {
        // SAFETY: a plain COM call on a live interface.
        unsafe { media_type.GetUINT32(key) }.unwrap()
    }

    fn u64_of(media_type: &IMFMediaType, key: &GUID) -> u64 {
        // SAFETY: as above.
        unsafe { media_type.GetUINT64(key) }.unwrap()
    }

    fn guid_of(media_type: &IMFMediaType, key: &GUID) -> GUID {
        // SAFETY: as above.
        unsafe { media_type.GetGUID(key) }.unwrap()
    }

    fn assert_bt709_limited(media_type: &IMFMediaType) {
        assert_eq!(
            [
                u32_of(media_type, &MF_MT_VIDEO_PRIMARIES),
                u32_of(media_type, &MF_MT_TRANSFER_FUNCTION),
                u32_of(media_type, &MF_MT_YUV_MATRIX),
                u32_of(media_type, &MF_MT_VIDEO_NOMINAL_RANGE),
            ],
            [
                MFVideoPrimaries_BT709.0 as u32,
                MFVideoTransFunc_709.0 as u32,
                MFVideoTransferMatrix_BT709.0 as u32,
                MFNominalRange_16_235.0 as u32,
            ]
        );
    }

    #[test]
    fn video_types_carry_the_decisions() {
        let _started = Started::new().unwrap();
        let rate = Rational::new(30_000, 1_001);
        let output = encoder_output(
            VideoCodec::Hevc,
            Profile::HevcMain10,
            64,
            36,
            rate,
            8_000_000,
        )
        .unwrap();
        assert_eq!(guid_of(&output, &MF_MT_MAJOR_TYPE), MFMediaType_Video);
        assert_eq!(guid_of(&output, &MF_MT_SUBTYPE), MFVideoFormat_HEVC);
        assert_eq!(u64_of(&output, &MF_MT_FRAME_SIZE), 64 << 32 | 36);
        assert_eq!(u64_of(&output, &MF_MT_FRAME_RATE), 30_000 << 32 | 1_001);
        assert_eq!(u64_of(&output, &MF_MT_PIXEL_ASPECT_RATIO), 1 << 32 | 1);
        assert_eq!(
            u32_of(&output, &MF_MT_INTERLACE_MODE),
            MFVideoInterlace_Progressive.0 as u32
        );
        assert_eq!(u32_of(&output, &MF_MT_AVG_BITRATE), 8_000_000);
        assert_eq!(u32_of(&output, &MF_MT_MPEG2_PROFILE), 2);
        assert_bt709_limited(&output);

        let input = encoder_input(PixelFormat::Rgba8, 64, 36, rate).unwrap();
        assert_eq!(guid_of(&input, &MF_MT_SUBTYPE), MFVideoFormat_NV12);
        assert_eq!(u64_of(&input, &MF_MT_FRAME_SIZE), 64 << 32 | 36);
        assert_bt709_limited(&input);
        assert_eq!(input_subtype(PixelFormat::Rgba16), MFVideoFormat_P010);
        assert_eq!(input_subtype(PixelFormat::P010), MFVideoFormat_P010);

        assert_eq!(
            [
                Profile::H264High,
                Profile::H264Main,
                Profile::H264Baseline,
                Profile::HevcMain,
                Profile::HevcMain10
            ]
            .map(profile_value),
            [100, 77, 66, 1, 2]
        );
    }

    #[test]
    fn audio_types_carry_the_decisions() {
        let _started = Started::new().unwrap();
        let output = aac_output(48_000, 2, 192_000).unwrap();
        assert_eq!(guid_of(&output, &MF_MT_MAJOR_TYPE), MFMediaType_Audio);
        assert_eq!(guid_of(&output, &MF_MT_SUBTYPE), MFAudioFormat_AAC);
        assert_eq!(u32_of(&output, &MF_MT_AAC_PAYLOAD_TYPE), 0);
        assert_eq!(u32_of(&output, &MF_MT_AUDIO_SAMPLES_PER_SECOND), 48_000);
        assert_eq!(u32_of(&output, &MF_MT_AUDIO_NUM_CHANNELS), 2);
        assert_eq!(u32_of(&output, &MF_MT_AUDIO_BITS_PER_SAMPLE), 16);
        assert_eq!(u32_of(&output, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND), 24_000);
        let input = pcm_input(44_100, 1).unwrap();
        assert_eq!(guid_of(&input, &MF_MT_SUBTYPE), MFAudioFormat_PCM);
        assert_eq!(u32_of(&input, &MF_MT_AUDIO_SAMPLES_PER_SECOND), 44_100);
        assert_eq!(u32_of(&input, &MF_MT_AUDIO_NUM_CHANNELS), 1);
        assert_eq!(u32_of(&input, &MF_MT_AUDIO_BITS_PER_SAMPLE), 16);
        assert_eq!(u32_of(&input, &MF_MT_AUDIO_BLOCK_ALIGNMENT), 2);
        assert_eq!(u32_of(&input, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND), 88_200);
    }
}
