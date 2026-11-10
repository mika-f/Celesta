//! `MFTEnumEx`: which encoders this system has.

use crate::components::Encoders;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, MFAudioFormat_AAC, MFAudioFormat_PCM, MFMediaType_Audio, MFMediaType_Video,
    MFT_CATEGORY_AUDIO_ENCODER, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ASYNCMFT,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_LOCALMFT, MFT_ENUM_FLAG_SORTANDFILTER,
    MFT_ENUM_FLAG_SYNCMFT, MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFVideoFormat_H264,
    MFVideoFormat_HEVC, MFVideoFormat_NV12,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows_core::GUID;

/// The flags that list hardware and software transforms, best first.
pub(crate) const HARDWARE_FLAGS: MFT_ENUM_FLAG = MFT_ENUM_FLAG(
    MFT_ENUM_FLAG_HARDWARE.0
        | MFT_ENUM_FLAG_SYNCMFT.0
        | MFT_ENUM_FLAG_ASYNCMFT.0
        | MFT_ENUM_FLAG_LOCALMFT.0
        | MFT_ENUM_FLAG_SORTANDFILTER.0,
);

/// Which encoders `MFTEnumEx` lists, hardware flags included. Media
/// Foundation must have started.
pub(crate) fn encoders() -> Encoders {
    let video = |output: GUID| {
        lists(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_NV12,
            },
            MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: output,
            },
        )
    };
    Encoders {
        h264: video(MFVideoFormat_H264),
        hevc: video(MFVideoFormat_HEVC),
        aac: lists(
            MFT_CATEGORY_AUDIO_ENCODER,
            MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Audio,
                guidSubtype: MFAudioFormat_PCM,
            },
            MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Audio,
                guidSubtype: MFAudioFormat_AAC,
            },
        ),
    }
}

/// Whether `MFTEnumEx` lists a transform of `category` from `input` to
/// `output`; a failed enumeration lists none.
fn lists(category: GUID, input: MFT_REGISTER_TYPE_INFO, output: MFT_REGISTER_TYPE_INFO) -> bool {
    let mut activates: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the type infos outlive the call, and `activates` and `count`
    // are its out-parameters.
    let listed = unsafe {
        MFTEnumEx(
            category,
            HARDWARE_FLAGS,
            Some(&input),
            Some(&output),
            &mut activates,
            &mut count,
        )
    };
    if listed.is_err() || activates.is_null() {
        return false;
    }
    // SAFETY: on success `activates` points to `count` interface pointers in
    // memory from `CoTaskMemAlloc`; dropping each releases it, and the array
    // is freed once.
    unsafe {
        for activate in std::slice::from_raw_parts_mut(activates, count as usize) {
            drop(activate.take());
        }
        CoTaskMemFree(Some(activates.cast_const().cast()));
    }
    count > 0
}
