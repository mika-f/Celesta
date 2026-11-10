//! The AAC encoder's `MF_MT_USER_DATA`: a `HEAACWAVEINFO`'s 12 bytes
//! (`wPayloadType`, `wAudioProfileLevelIndication`, `wStructType`, and
//! reserved fields) and then the AudioSpecificConfig.
//!
//! `aac_codec_string` is copied from `plugins/avfoundation/src/esds.rs`
//! (P5), which is private to that crate.

use celesta_media::MediaError;

/// The bytes of `HEAACWAVEINFO` before the AudioSpecificConfig.
const HEAAC_WAVE_INFO: usize = 12;

fn malformed(what: &str) -> MediaError {
    MediaError::Codec(format!("Media Foundation gave a malformed {what}"))
}

/// The AudioSpecificConfig in an AAC output type's user data: what follows
/// the 12 `HEAACWAVEINFO` bytes, which must not be empty.
pub(crate) fn asc_from_user_data(user_data: &[u8]) -> Result<Vec<u8>, MediaError> {
    match user_data.get(HEAAC_WAVE_INFO..) {
        Some(asc) if !asc.is_empty() => Ok(asc.to_vec()),
        _ => Err(malformed("AAC user data")),
    }
}

/// User data for `asc`: raw payload (type 0), profile-level `0xFE` (none
/// given), struct type 0, the reserved fields zero, then `asc`.
pub(crate) fn user_data_from_asc(asc: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(HEAAC_WAVE_INFO + asc.len());
    data.extend_from_slice(&0u16.to_le_bytes()); // wPayloadType
    data.extend_from_slice(&0xFEu16.to_le_bytes()); // wAudioProfileLevelIndication
    data.extend_from_slice(&0u16.to_le_bytes()); // wStructType
    data.extend_from_slice(&[0; 6]); // wReserved1, dwReserved2
    data.extend_from_slice(asc);
    data
}

/// `mp4a.40.<audio object type>` for an AudioSpecificConfig.
pub(crate) fn aac_codec_string(asc: &[u8]) -> Result<String, MediaError> {
    let first = *asc
        .first()
        .ok_or_else(|| malformed("AAC AudioSpecificConfig"))?;
    let mut object_type = u32::from(first >> 3);
    if object_type == 31 {
        // The escape: 32 plus the next six bits.
        let second = *asc
            .get(1)
            .ok_or_else(|| malformed("AAC AudioSpecificConfig"))?;
        object_type = 32 + u32::from((first & 7) << 3 | second >> 5);
    }
    Ok(format!("mp4a.40.{object_type}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_holds_the_asc() {
        let data = user_data_from_asc(&[0x11, 0x90]);
        assert_eq!(data.len(), 14);
        assert_eq!(data[..12], [0, 0, 0xFE, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(asc_from_user_data(&data).unwrap(), [0x11, 0x90]);
    }

    #[test]
    fn short_user_data_is_refused() {
        assert!(matches!(
            asc_from_user_data(&[0; 11]),
            Err(MediaError::Codec(_))
        ));
        // Twelve bytes carry no AudioSpecificConfig.
        assert!(asc_from_user_data(&[0; 12]).is_err());
        assert!(asc_from_user_data(&[]).is_err());
    }

    #[test]
    fn the_codec_string_names_the_object_type() {
        assert_eq!(aac_codec_string(&[0x11, 0x90]).unwrap(), "mp4a.40.2");
        // The escape: 31, then 32 + 5 (object type 37).
        assert_eq!(aac_codec_string(&[0xF8, 0xA0]).unwrap(), "mp4a.40.37");
        assert!(aac_codec_string(&[]).is_err());
        assert!(aac_codec_string(&[0xF8]).is_err());
    }
}
