//! `MFTEnumEx`: which encoders this system has.

use crate::components::Encoders;
use crate::mf::mft::candidates;
use windows::Win32::Media::MediaFoundation::{
    MFAudioFormat_AAC, MFAudioFormat_PCM, MFT_CATEGORY_AUDIO_ENCODER, MFT_CATEGORY_VIDEO_ENCODER,
    MFVideoFormat_H264, MFVideoFormat_HEVC, MFVideoFormat_NV12,
};
use windows_core::GUID;

/// Which encoders `MFTEnumEx` lists, hardware flags included. Media
/// Foundation must have started.
pub(crate) fn encoders() -> Encoders {
    Encoders {
        h264: lists(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFVideoFormat_NV12,
            MFVideoFormat_H264,
        ),
        hevc: lists(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFVideoFormat_NV12,
            MFVideoFormat_HEVC,
        ),
        aac: lists(
            MFT_CATEGORY_AUDIO_ENCODER,
            MFAudioFormat_PCM,
            MFAudioFormat_AAC,
        ),
    }
}

/// Whether `MFTEnumEx` lists a transform of `category` from `input` to
/// `output`, with the hardware flags; a failed enumeration lists none.
fn lists(category: GUID, input: GUID, output: GUID) -> bool {
    candidates(category, input, output, true).is_ok_and(|listed| !listed.is_empty())
}
