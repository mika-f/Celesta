//! What Apple's frameworks have on this Mac, for `describe`: the encoders
//! VideoToolbox and AudioToolbox list, and the sample rates the AAC encoder
//! takes. A list a framework will not give is empty, which leaves its
//! components unavailable with their reasons.

use crate::components::AAC_FORMAT;
use objc2_audio_toolbox::{
    AudioFormatGetProperty, AudioFormatGetPropertyInfo, AudioFormatPropertyID,
    kAudioFormatProperty_AvailableEncodeSampleRates, kAudioFormatProperty_Encoders,
};
use objc2_core_audio_types::{AudioClassDescription, AudioValueRange};
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_video_toolbox::{VTCopyVideoEncoderList, kVTVideoEncoderList_CodecType};
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{NonNull, null};

/// The codec type (`CMVideoCodecType`) of every encoder
/// `VTCopyVideoEncoderList` lists, hardware and software, in its order.
pub(crate) fn video_encoder_types() -> Vec<u32> {
    let mut list: *const CFArray = null();
    // SAFETY: `list` is a valid place for the array's pointer; no options.
    let status = unsafe { VTCopyVideoEncoderList(None, NonNull::from(&mut list)) };
    let Some(list) = NonNull::new(list.cast_mut()) else {
        return Vec::new();
    };
    // SAFETY: the array is the caller's to release (the Copy rule), and is
    // one of dictionaries keyed by strings.
    let list = unsafe {
        CFRetained::cast_unchecked::<CFArray<CFDictionary<CFString, CFType>>>(CFRetained::from_raw(
            list,
        ))
    };
    if status != 0 {
        return Vec::new();
    }
    // SAFETY: an immutable static CFString VideoToolbox defines.
    let key = unsafe { kVTVideoEncoderList_CodecType };
    list.iter()
        .filter_map(|encoder| {
            let codec_type = encoder.get(key)?.downcast::<CFNumber>().ok()?.as_i64()?;
            u32::try_from(codec_type).ok()
        })
        .collect()
}

/// The format id of every encoder `kAudioFormatProperty_Encoders` lists for
/// the formats the plugin encodes (AAC's alone). The property takes the
/// format as its specifier and answers the installed encoders' class
/// descriptions, whose subtype is the format id each encodes.
pub(crate) fn audio_encoder_formats() -> Vec<u32> {
    let none = AudioClassDescription {
        mType: 0,
        mSubType: 0,
        mManufacturer: 0,
    };
    format_property(kAudioFormatProperty_Encoders, Some(AAC_FORMAT), none)
        .iter()
        .map(|encoder| encoder.mSubType)
        .collect()
}

/// The sample rates AudioToolbox's AAC encoder takes, ascending:
/// `kAudioFormatProperty_AvailableEncodeSampleRates`' ranges, each by its
/// minimum.
pub(crate) fn aac_sample_rates() -> Vec<u32> {
    let none = AudioValueRange {
        mMinimum: 0.0,
        mMaximum: 0.0,
    };
    let ranges = format_property(
        kAudioFormatProperty_AvailableEncodeSampleRates,
        Some(AAC_FORMAT),
        none,
    );
    let mut rates: Vec<u32> = ranges
        .iter()
        .filter(|range| range.mMinimum.is_finite() && range.mMinimum >= 1.0)
        .map(|range| range.mMinimum.round() as u32)
        .collect();
    rates.sort_unstable();
    rates.dedup();
    rates
}

/// The values of AudioToolbox's `property`, a list of `T`, for the format id
/// `specifier` when the property takes one; none when it fails. `empty` is
/// any `T`, which the buffer is filled with before AudioToolbox writes it.
fn format_property<T: Copy>(
    property: AudioFormatPropertyID,
    specifier: Option<u32>,
    empty: T,
) -> Vec<T> {
    let (specifier_size, specifier): (u32, *const c_void) = match &specifier {
        Some(format) => (size_of::<u32>() as u32, (format as *const u32).cast()),
        None => (0, null()),
    };
    let mut size: u32 = 0;
    // SAFETY: `specifier` is null or points at `specifier_size` bytes that
    // outlive the call; `size` is a valid place for the answer.
    let status = unsafe {
        AudioFormatGetPropertyInfo(
            property,
            specifier_size,
            specifier,
            NonNull::from(&mut size),
        )
    };
    if status != 0 || size == 0 {
        return Vec::new();
    }
    let mut values = vec![empty; size as usize / size_of::<T>()];
    let mut size = (values.len() * size_of::<T>()) as u32;
    // SAFETY: `values` has room for `size` bytes, and the property's data is
    // a list of `T`, which is plain old data.
    let status = unsafe {
        AudioFormatGetProperty(
            property,
            specifier_size,
            specifier,
            &mut size,
            values.as_mut_ptr().cast::<c_void>(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    values.truncate(size as usize / size_of::<T>());
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::VideoCodec;

    /// AudioToolbox's AAC encoder and VideoToolbox's H.264 encoder have
    /// software implementations on every macOS, so their absence is a bug in
    /// how the lists are read, never a Mac without them.
    #[test]
    fn every_mac_lists_the_aac_and_h264_encoders() {
        let audio = audio_encoder_formats();
        assert!(audio.contains(&AAC_FORMAT), "{audio:x?}");
        let video = video_encoder_types();
        assert!(video.contains(&VideoCodec::H264.codec_type()), "{video:x?}");
    }

    #[test]
    fn aac_lists_the_rates_it_takes() {
        let rates = aac_sample_rates();
        assert!(rates.windows(2).all(|pair| pair[0] < pair[1]), "{rates:?}");
        for rate in [44_100, 48_000] {
            assert!(rates.contains(&rate), "{rates:?}");
        }
    }

    #[test]
    fn a_property_that_fails_is_empty() {
        assert!(format_property(u32::from_be_bytes(*b"none"), None, 0_u32).is_empty());
    }
}
