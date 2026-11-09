//! The AAC encoder's magic cookie: an MPEG-4 `ES_Descriptor` (ISO/IEC
//! 14496-1) whose `DecoderSpecificInfo` is the AudioSpecificConfig.

use celesta_media::MediaError;

const ES_DESCRIPTOR: u8 = 3;
const DECODER_CONFIG: u8 = 4;
const DECODER_SPECIFIC_INFO: u8 = 5;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the AVFoundation muxers write cookies in a later task"
    )
)]
const SL_CONFIG: u8 = 6;

fn malformed() -> MediaError {
    MediaError::Codec("AudioToolbox gave a malformed AAC magic cookie".to_owned())
}

/// The first descriptor in `data`: its tag, its body, and what follows. The
/// length is 1 to 4 bytes of 7 bits each, the high bit set on all but the last.
fn descriptor(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = data.split_first()?;
    let mut length = 0usize;
    let mut index = 0;
    loop {
        let byte = *rest.get(index)?;
        length = length << 7 | usize::from(byte & 0x7F);
        index += 1;
        if byte & 0x80 == 0 {
            break;
        }
        if index == 4 {
            return None;
        }
    }
    let body = rest.get(index..index + length)?;
    Some((tag, body, &rest[index + length..]))
}

/// The body of the first descriptor tagged `tag` in a sequence of them.
fn find(mut data: &[u8], tag: u8) -> Option<&[u8]> {
    while !data.is_empty() {
        let (found, body, rest) = descriptor(data)?;
        if found == tag {
            return Some(body);
        }
        data = rest;
    }
    None
}

/// The AudioSpecificConfig in an AAC magic cookie: the `DecoderSpecificInfo`
/// inside the `DecoderConfigDescriptor` of the `ES_Descriptor`.
pub(crate) fn asc_from_cookie(cookie: &[u8]) -> Result<Vec<u8>, MediaError> {
    let es = find(cookie, ES_DESCRIPTOR).ok_or_else(malformed)?;
    // ES_ID, then the flags saying which optional fields follow.
    let flags = *es.get(2).ok_or_else(malformed)?;
    let mut skip = 3;
    if flags & 0x80 != 0 {
        skip += 2; // dependsOn_ES_ID
    }
    if flags & 0x40 != 0 {
        skip += 1 + usize::from(*es.get(skip).ok_or_else(malformed)?); // URL
    }
    if flags & 0x20 != 0 {
        skip += 2; // OCR_ES_Id
    }
    let config =
        find(es.get(skip..).ok_or_else(malformed)?, DECODER_CONFIG).ok_or_else(malformed)?;
    // objectTypeIndication, streamType, bufferSizeDB, maxBitrate, avgBitrate.
    let info = find(
        config.get(13..).ok_or_else(malformed)?,
        DECODER_SPECIFIC_INFO,
    )
    .ok_or_else(malformed)?;
    Ok(info.to_vec())
}

/// `tag`, `body`'s length as AudioToolbox writes it (four bytes, the first
/// three with the high bit set), then `body`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the AVFoundation muxers write cookies in a later task"
    )
)]
fn write(out: &mut Vec<u8>, tag: u8, body: &[u8]) {
    let length = body.len() as u32;
    out.push(tag);
    out.extend_from_slice(&[
        0x80 | (length >> 21 & 0x7F) as u8,
        0x80 | (length >> 14 & 0x7F) as u8,
        0x80 | (length >> 7 & 0x7F) as u8,
        (length & 0x7F) as u8,
    ]);
    out.extend_from_slice(body);
}

/// An AAC magic cookie for `asc`: an `ES_Descriptor` with ES_ID 0, a
/// `DecoderConfigDescriptor` (MPEG-4 audio, audio stream, `bitrate` as both
/// maximum and average), the `DecoderSpecificInfo`, and an `SLConfig`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the AVFoundation muxers write cookies in a later task"
    )
)]
pub(crate) fn cookie_from_asc(asc: &[u8], bitrate: u32) -> Vec<u8> {
    let mut info = Vec::new();
    write(&mut info, DECODER_SPECIFIC_INFO, asc);
    let mut config = vec![0x40, 0x15, 0, 0, 0];
    config.extend_from_slice(&bitrate.to_be_bytes());
    config.extend_from_slice(&bitrate.to_be_bytes());
    config.extend_from_slice(&info);
    let mut es = vec![0, 0, 0];
    write(&mut es, DECODER_CONFIG, &config);
    write(&mut es, SL_CONFIG, &[2]);
    let mut cookie = Vec::new();
    write(&mut cookie, ES_DESCRIPTOR, &es);
    cookie
}

/// `mp4a.40.<audio object type>` for an AudioSpecificConfig.
pub(crate) fn aac_codec_string(asc: &[u8]) -> Result<String, MediaError> {
    let malformed = || MediaError::Codec("the AAC AudioSpecificConfig is malformed".to_owned());
    let first = *asc.first().ok_or_else(malformed)?;
    let mut object_type = u32::from(first >> 3);
    if object_type == 31 {
        // The escape: 32 plus the next six bits.
        let second = *asc.get(1).ok_or_else(malformed)?;
        object_type = 32 + u32::from((first & 7) << 3 | second >> 5);
    }
    Ok(format!("mp4a.40.{object_type}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_cookie_holds_the_asc() {
        let cookie =
            hex("03808080220000000480808014401400180000000b300002ee0005808080021190068080800102");
        assert_eq!(asc_from_cookie(&cookie).unwrap(), [0x11, 0x90]);
        assert_eq!(aac_codec_string(&[0x11, 0x90]).unwrap(), "mp4a.40.2");
        assert_eq!(
            asc_from_cookie(&cookie_from_asc(&[0x11, 0x90], 192_000)).unwrap(),
            [0x11, 0x90]
        );
        // AudioToolbox's own bytes are what cookie_from_asc writes, but for
        // the rates and the stream type.
        let made = cookie_from_asc(&[0x11, 0x90], 192_000);
        assert_eq!(made[..8], cookie[..8]);
        assert_eq!(made[made.len() - 6..], cookie[cookie.len() - 6..]);
        // 1-byte lengths.
        let short = hex(&format!(
            "031900000004114015{}05021190060102",
            "00".repeat(11)
        ));
        assert_eq!(asc_from_cookie(&short).unwrap(), [0x11, 0x90]);
        // No DecoderSpecificInfo.
        let none = hex(&format!("0312000000040d4015{}", "00".repeat(11)));
        assert!(asc_from_cookie(&none).is_err());
        assert!(asc_from_cookie(&[]).is_err());
        assert!(aac_codec_string(&[]).is_err());
    }
}
