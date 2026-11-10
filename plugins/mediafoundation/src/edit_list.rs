//! Each track's edit list in an MP4 or MOV, which Media Foundation's source
//! reader ignores (Task 1): the decoder applies it itself (Decisions,
//! Decoder).
//!
//! MP4 and MOV share the boxes read: `moov/mvhd`'s timescale, and for each
//! `trak` its `mdia/hdlr` handler type, `mdia/mdhd` timescale and
//! duration, and `edts/elst`. The top-level boxes are walked, seeking past `mdat` and the
//! rest, and only `moov` is read. It takes any `Read + Seek`, so it is
//! tested everywhere.

use std::io::{self, Read, Seek, SeekFrom};

/// What a track's edit list does to it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrackEdit {
    /// `hdlr`'s handler type: `*b"vide"`, `*b"soun"`, ….
    pub(crate) handler: [u8; 4],
    /// `mdhd`'s timescale, which `media_time` is in.
    pub(crate) timescale: u32,
    /// Where the track's presentation starts in its media: the first
    /// non-empty edit's media time, in `mdhd` units; 0 without `edts`.
    pub(crate) media_time: i64,
    /// Seconds of empty edits (media time −1) before that edit, from their
    /// `mvhd`-unit durations: how late the track starts; 0.0 without.
    pub(crate) start_delay: f64,
    /// Seconds of that edit's `segment_duration` (`mvhd` units): how much of
    /// the media is shown; `None` without `edts` or when it is 0.
    pub(crate) duration: Option<f64>,
    /// `mdhd`'s duration: how long the media is, in `mdhd` units; `None`
    /// when it says 0 or "unknown" (all ones). An edit's `duration` is in
    /// the coarser `mvhd` units, so the decoder also stops at the media's
    /// end.
    pub(crate) media_duration: Option<i64>,
}

fn malformed(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("malformed {what}"))
}

/// One box in a byte slice: its type and its payload.
struct Child<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

/// The boxes laid end to end in `bytes`; a box running past the end is cut
/// at it.
fn children(mut bytes: &[u8]) -> impl Iterator<Item = Child<'_>> {
    std::iter::from_fn(move || {
        if bytes.len() < 8 {
            return None;
        }
        let size = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
        let kind: [u8; 4] = bytes[4..8].try_into().ok()?;
        let (header, size) = match size {
            0 => (8, bytes.len() as u64),
            1 => {
                let large = u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?);
                (16, large)
            }
            size => (8, u64::from(size)),
        };
        if size < header as u64 {
            bytes = &[];
            return None;
        }
        let end = usize::try_from(size).unwrap_or(usize::MAX).min(bytes.len());
        let payload = &bytes[header.min(end)..end];
        bytes = &bytes[end..];
        Some(Child { kind, payload })
    })
}

/// The payload of the first child of `bytes` of type `kind`.
fn child<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(bytes)
        .find(|child| &child.kind == kind)
        .map(|child| child.payload)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// The timescale and duration of a full box laid out as `mvhd` and `mdhd`
/// are: version and flags, then creation and modification times (32 or 64
/// bits by the version), then the timescale and the duration (likewise).
fn timescale_and_duration(payload: &[u8]) -> Option<(u32, Option<i64>)> {
    let long = *payload.first()? == 1;
    let at = if long { 20 } else { 12 };
    let timescale = u32_at(payload, at).filter(|timescale| *timescale > 0)?;
    let duration = if long {
        u64_at(payload, at + 4).filter(|duration| *duration != u64::MAX)
    } else {
        u32_at(payload, at + 4)
            .filter(|duration| *duration != u32::MAX)
            .map(u64::from)
    };
    let duration = duration
        .filter(|duration| *duration > 0)
        .and_then(|duration| i64::try_from(duration).ok());
    Some((timescale, duration))
}

/// `elst`'s entries: each one's `segment_duration` and `media_time`.
fn edit_entries(payload: &[u8]) -> Option<Vec<(u64, i64)>> {
    let version = *payload.first()?;
    let count = u32_at(payload, 4)? as usize;
    let size = if version == 1 { 20 } else { 12 };
    let entries = payload.get(8..)?;
    if entries.len() / size < count {
        return None;
    }
    Some(
        entries
            .chunks_exact(size)
            .take(count)
            .map(|entry| {
                if version == 1 {
                    (
                        u64_at(entry, 0).unwrap_or(0),
                        u64_at(entry, 8).unwrap_or(0) as i64,
                    )
                } else {
                    (
                        u64::from(u32_at(entry, 0).unwrap_or(0)),
                        i64::from(u32_at(entry, 4).unwrap_or(0) as i32),
                    )
                }
            })
            .collect(),
    )
}

/// The edit of `trak`'s payload, whose movie timescale is `movie`.
fn track_edit(trak: &[u8], movie: u32) -> Option<TrackEdit> {
    let mdia = child(trak, b"mdia")?;
    let handler: [u8; 4] = child(mdia, b"hdlr")?.get(8..12)?.try_into().ok()?;
    let (timescale, media_duration) = timescale_and_duration(child(mdia, b"mdhd")?)?;
    let mut edit = TrackEdit {
        handler,
        timescale,
        media_time: 0,
        start_delay: 0.0,
        duration: None,
        media_duration,
    };
    let Some(elst) = child(trak, b"edts").and_then(|edts| child(edts, b"elst")) else {
        return Some(edit);
    };
    let seconds = |duration: u64| duration as f64 / f64::from(movie);
    for (segment_duration, media_time) in edit_entries(elst)? {
        if media_time == -1 {
            edit.start_delay += seconds(segment_duration);
        } else {
            edit.media_time = media_time;
            edit.duration = (segment_duration > 0).then(|| seconds(segment_duration));
            break;
        }
    }
    Some(edit)
}

/// The edit of every `trak` in `file`'s `moov`, in file order; none when it
/// has no `moov`. Only `moov` is read: every other top-level box is sought
/// past.
pub(crate) fn track_edits(file: &mut (impl Read + Seek)) -> io::Result<Vec<TrackEdit>> {
    file.seek(SeekFrom::Start(0))?;
    loop {
        let mut header = [0u8; 8];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(Vec::new()),
            Err(error) => return Err(error),
        }
        let size = u32::from_be_bytes(header[0..4].try_into().expect("four bytes"));
        let kind = &header[4..8];
        // The payload's length, or None to the end of the file.
        let length = match size {
            0 => None,
            1 => {
                let mut large = [0u8; 8];
                file.read_exact(&mut large)?;
                Some(
                    u64::from_be_bytes(large)
                        .checked_sub(16)
                        .ok_or_else(|| malformed("box size"))?,
                )
            }
            size => Some(
                u64::from(size)
                    .checked_sub(8)
                    .ok_or_else(|| malformed("box size"))?,
            ),
        };
        if kind == b"moov" {
            let mut moov = Vec::new();
            file.take(length.unwrap_or(u64::MAX))
                .read_to_end(&mut moov)?;
            let (movie, _) = child(&moov, b"mvhd")
                .and_then(timescale_and_duration)
                .ok_or_else(|| malformed("mvhd"))?;
            return Ok(children(&moov)
                .filter(|child| &child.kind == b"trak")
                .filter_map(|trak| track_edit(trak.payload, movie))
                .collect());
        }
        match length {
            None => return Ok(Vec::new()),
            Some(length) => {
                let skip = i64::try_from(length).map_err(|_| malformed("box size"))?;
                file.seek(SeekFrom::Current(skip))?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A box of `kind` around `payload`.
    fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend(kind);
        bytes.extend(payload);
        bytes
    }

    /// A full box (version 0, no flags) of `kind` around `payload`.
    fn full(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0; 4];
        bytes.extend(payload);
        atom(kind, &bytes)
    }

    /// `mvhd` or `mdhd` at `timescale` lasting `duration`, version 0.
    fn header(kind: &[u8; 4], timescale: u32, duration: u32) -> Vec<u8> {
        let mut payload = vec![0; 8];
        payload.extend(timescale.to_be_bytes());
        payload.extend(duration.to_be_bytes());
        // The rest of the box, which is not read.
        payload.extend([0; 16]);
        full(kind, &payload)
    }

    /// `mdhd` at `timescale` lasting `duration`, version 1 (64-bit times).
    fn header_v1(timescale: u32, duration: u64) -> Vec<u8> {
        let mut payload = vec![1, 0, 0, 0];
        payload.extend([0; 16]);
        payload.extend(timescale.to_be_bytes());
        payload.extend(duration.to_be_bytes());
        atom(b"mdhd", &payload)
    }

    /// An `elst` of version 0 entries `(segment_duration, media_time)`.
    fn elst(entries: &[(u32, i32)]) -> Vec<u8> {
        let mut payload = (entries.len() as u32).to_be_bytes().to_vec();
        for (duration, time) in entries {
            payload.extend(duration.to_be_bytes());
            payload.extend(time.to_be_bytes());
            payload.extend([0, 1, 0, 0]);
        }
        full(b"elst", &payload)
    }

    /// A `trak` of `handler` at `timescale`, with `edits` when some.
    fn trak(handler: &[u8; 4], mdhd: Vec<u8>, edits: Option<Vec<u8>>) -> Vec<u8> {
        let mut hdlr = vec![0; 4];
        hdlr.extend(handler);
        hdlr.extend([0; 12]);
        hdlr.push(0);
        let mut mdia = mdhd;
        mdia.extend(full(b"hdlr", &hdlr));
        let mut payload = full(b"tkhd", &[0; 80]);
        if let Some(edits) = edits {
            payload.extend(atom(b"edts", &edits));
        }
        payload.extend(atom(b"mdia", &mdia));
        atom(b"trak", &payload)
    }

    /// A `moov` at movie timescale `movie` of `traks`.
    fn moov(movie: u32, traks: &[Vec<u8>]) -> Vec<u8> {
        let mut payload = header(b"mvhd", movie, 0);
        for trak in traks {
            payload.extend(trak);
        }
        atom(b"moov", &payload)
    }

    fn ftyp(brand: &[u8; 4]) -> Vec<u8> {
        let mut payload = brand.to_vec();
        payload.extend([0, 0, 2, 0]);
        payload.extend(brand);
        atom(b"ftyp", &payload)
    }

    fn edits_of(file: Vec<u8>) -> Vec<TrackEdit> {
        track_edits(&mut Cursor::new(file)).unwrap()
    }

    /// The test files' two tracks: video at 30000 with an empty edit of half
    /// a second then media time 2002, and audio at 48000 from 1024.
    fn two_tracks() -> Vec<u8> {
        moov(
            1_000,
            &[
                trak(
                    b"vide",
                    header(b"mdhd", 30_000, 302_302),
                    Some(elst(&[(500, -1), (10_000, 2_002)])),
                ),
                trak(
                    b"soun",
                    header_v1(48_000, 49_024),
                    Some(elst(&[(1_000, 1_024)])),
                ),
            ],
        )
    }

    fn expected_two_tracks() -> Vec<TrackEdit> {
        vec![
            TrackEdit {
                handler: *b"vide",
                timescale: 30_000,
                media_time: 2_002,
                start_delay: 0.5,
                duration: Some(10.0),
                media_duration: Some(302_302),
            },
            TrackEdit {
                handler: *b"soun",
                timescale: 48_000,
                media_time: 1_024,
                start_delay: 0.0,
                duration: Some(1.0),
                media_duration: Some(49_024),
            },
        ]
    }

    #[test]
    fn an_edit_list_gives_its_media_time() {
        let mut file = ftyp(b"isom");
        file.extend(moov(
            1_000,
            &[trak(
                b"soun",
                header(b"mdhd", 48_000, 49_024),
                Some(elst(&[(1_000, 1_024)])),
            )],
        ));
        assert_eq!(
            edits_of(file),
            [TrackEdit {
                handler: *b"soun",
                timescale: 48_000,
                media_time: 1_024,
                start_delay: 0.0,
                duration: Some(1.0),
                media_duration: Some(49_024),
            }]
        );
    }

    #[test]
    fn an_empty_edit_delays_the_start() {
        let mut file = ftyp(b"isom");
        file.extend(moov(
            1_000,
            &[trak(
                b"vide",
                header(b"mdhd", 30_000, u32::MAX),
                Some(elst(&[(500, -1), (2_000, 2_002)])),
            )],
        ));
        let edits = edits_of(file);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].start_delay, 0.5);
        assert_eq!(edits[0].media_time, 2_002);
        assert_eq!(edits[0].timescale, 30_000);
        assert_eq!(edits[0].duration, Some(2.0));
        // All ones: unknown.
        assert_eq!(edits[0].media_duration, None);
    }

    #[test]
    fn no_edit_list_changes_nothing() {
        let mut file = ftyp(b"isom");
        file.extend(moov(
            600,
            &[trak(b"vide", header(b"mdhd", 10_240, 0), None)],
        ));
        assert_eq!(
            edits_of(file),
            [TrackEdit {
                handler: *b"vide",
                timescale: 10_240,
                media_time: 0,
                start_delay: 0.0,
                duration: None,
                media_duration: None,
            }]
        );
    }

    #[test]
    fn a_moov_after_a_large_mdat_reads_the_same() {
        let mut file = ftyp(b"isom");
        // An `mdat` with a 64-bit size, as a long recording's.
        let data = vec![0x5a; 100_000];
        file.extend(1u32.to_be_bytes());
        file.extend(b"mdat");
        file.extend((data.len() as u64 + 16).to_be_bytes());
        file.extend(&data);
        file.extend(atom(b"free", &[0; 32]));
        file.extend(two_tracks());
        assert_eq!(edits_of(file), expected_two_tracks());
    }

    #[test]
    fn a_movs_moov_reads_the_same() {
        // QuickTime's layout: `wide`, then `mdat`, then `moov`, whose `hdlr`
        // carries a component type (`mhlr`) where MP4 has zeros.
        let mut file = ftyp(b"qt  ");
        file.extend(atom(b"wide", &[]));
        file.extend(atom(b"mdat", &[0; 4_096]));
        let mut moov_bytes = two_tracks();
        let hdlr = moov_bytes
            .windows(4)
            .position(|window| window == b"hdlr")
            .unwrap();
        moov_bytes[hdlr + 8..hdlr + 12].copy_from_slice(b"mhlr");
        file.extend(moov_bytes);
        assert_eq!(edits_of(file), expected_two_tracks());
    }

    #[test]
    fn a_file_without_a_moov_has_no_edits() {
        assert_eq!(edits_of(b"not a movie\n".to_vec()), []);
        assert_eq!(edits_of(Vec::new()), []);
        let mut file = ftyp(b"isom");
        file.extend(atom(b"mdat", &[0; 64]));
        assert_eq!(edits_of(file), []);
    }

    #[test]
    fn a_malformed_box_is_an_error() {
        // A size smaller than its own header.
        let mut file = ftyp(b"isom");
        file.extend(4u32.to_be_bytes());
        file.extend(b"mdat");
        let error = track_edits(&mut Cursor::new(file)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
