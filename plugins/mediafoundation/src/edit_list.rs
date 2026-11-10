//! Each track's edit list in an MP4 or MOV, which Media Foundation's source
//! reader ignores (Task 1): the decoder applies it itself (Decisions,
//! Decoder).
//!
//! MP4 and MOV share the boxes read: `moov/mvhd`'s timescale, and for each
//! `trak` its `mdia/hdlr` handler type, `mdia/mdhd` timescale and
//! duration, `edts/elst`, and its first samples' times in
//! `mdia/minf/stbl/stts` and `ctts`. The top-level boxes are walked, seeking past `mdat` and the
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
    /// When the track's first shown sample is shown, in `mdhd` units: the
    /// earliest composition time of its first [`FIRST_SAMPLES`] samples;
    /// `None` without `stts` samples (a fragmented file), or without `mdhd`.
    /// The decoder compares it with the reader's time for the same frame.
    pub(crate) first_composition: Option<i64>,
}

/// How many of a track's first samples [`TrackEdit::first_composition`]
/// looks at: more than any reorder depth.
pub(crate) const FIRST_SAMPLES: usize = 64;

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

/// The `(count, value)` runs of `stts` or `ctts`'s payload, the value read
/// as signed when `signed` (as FFmpeg reads `ctts` of either version).
fn runs(payload: &[u8], signed: bool) -> Option<Vec<(u32, i64)>> {
    let count = u32_at(payload, 4)? as usize;
    let entries = payload.get(8..)?;
    if entries.len() / 8 < count {
        return None;
    }
    Some(
        entries
            .as_chunks::<8>()
            .0
            .iter()
            .take(count)
            .map(|entry| {
                let [count, value] = [&entry[..4], &entry[4..]]
                    .map(|half| u32::from_be_bytes(half.try_into().expect("four bytes")));
                let value = if signed {
                    i64::from(value as i32)
                } else {
                    i64::from(value)
                };
                (count, value)
            })
            .collect(),
    )
}

/// The earliest composition time of `stbl`'s first [`FIRST_SAMPLES`]
/// samples: their decode times from `stts`, from 0, plus `ctts`'s offsets
/// (none without `ctts`). `None` without samples, or with a malformed
/// `stts` or `ctts`.
fn first_composition(stbl: &[u8]) -> Option<i64> {
    let mut decode = Vec::with_capacity(FIRST_SAMPLES);
    let mut time = 0i64;
    'stts: for (count, delta) in runs(child(stbl, b"stts")?, false)? {
        for _ in 0..count {
            if decode.len() == FIRST_SAMPLES {
                break 'stts;
            }
            decode.push(time);
            time = time.saturating_add(delta);
        }
    }
    let mut offsets = Vec::with_capacity(decode.len());
    if let Some(ctts) = child(stbl, b"ctts") {
        'ctts: for (count, offset) in runs(ctts, true)? {
            for _ in 0..count {
                if offsets.len() == decode.len() {
                    break 'ctts;
                }
                offsets.push(offset);
            }
        }
    }
    offsets.resize(decode.len(), 0);
    decode
        .iter()
        .zip(&offsets)
        .map(|(decode, offset)| decode.saturating_add(*offset))
        .min()
}

/// The edit of `trak`'s payload, whose movie timescale is `movie`; none
/// when the track has no handler type, so cannot be told video from sound.
/// A track whose `mdhd` or `elst` is malformed is kept, with no edit, so
/// the tracks after it still line up with the reader's streams.
fn track_edit(trak: &[u8], movie: u32) -> Option<TrackEdit> {
    let mdia = child(trak, b"mdia")?;
    let handler: [u8; 4] = child(mdia, b"hdlr")?.get(8..12)?.try_into().ok()?;
    let mut edit = TrackEdit {
        handler,
        timescale: movie,
        media_time: 0,
        start_delay: 0.0,
        duration: None,
        media_duration: None,
        first_composition: None,
    };
    let Some((timescale, media_duration)) = child(mdia, b"mdhd").and_then(timescale_and_duration)
    else {
        return Some(edit);
    };
    edit.timescale = timescale;
    edit.media_duration = media_duration;
    edit.first_composition = child(mdia, b"minf")
        .and_then(|minf| child(minf, b"stbl"))
        .and_then(first_composition);
    let Some(entries) = child(trak, b"edts")
        .and_then(|edts| child(edts, b"elst"))
        .and_then(edit_entries)
    else {
        return Some(edit);
    };
    let seconds = |duration: u64| duration as f64 / f64::from(movie);
    for (segment_duration, media_time) in entries {
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
/// has no `moov`, or when a `trak` has no handler type (the decoder matches
/// edits to streams by their order among each kind, which such a track
/// would shift). Only `moov` is read: every other top-level box is sought
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
                .map(|trak| track_edit(trak.payload, movie))
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default());
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
                first_composition: None,
            },
            TrackEdit {
                handler: *b"soun",
                timescale: 48_000,
                media_time: 1_024,
                start_delay: 0.0,
                duration: Some(1.0),
                media_duration: Some(49_024),
                first_composition: None,
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
                first_composition: None,
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
                first_composition: None,
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
    fn a_malformed_track_keeps_its_place() {
        // The first sound track's `elst` says two entries and holds one.
        let mut broken = elst(&[(1_000, 2_048)]);
        let count = broken.windows(4).position(|w| w == b"elst").unwrap() + 8;
        broken[count..count + 4].copy_from_slice(&2u32.to_be_bytes());
        let mut file = ftyp(b"isom");
        file.extend(moov(
            1_000,
            &[
                trak(b"soun", header(b"mdhd", 48_000, 0), Some(broken)),
                trak(
                    b"soun",
                    header(b"mdhd", 48_000, 0),
                    Some(elst(&[(1_000, 1_024)])),
                ),
            ],
        ));
        let edits = edits_of(file);
        assert_eq!(edits.len(), 2);
        assert_eq!((edits[0].media_time, edits[0].duration), (0, None));
        assert_eq!(edits[1].media_time, 1_024);
        assert_eq!(edits[1].duration, Some(1.0));
    }

    #[test]
    fn a_track_without_a_handler_drops_every_edit() {
        let mut nameless = full(b"tkhd", &[0; 80]);
        nameless.extend(atom(b"edts", &elst(&[(1_000, 1_024)])));
        nameless.extend(atom(b"mdia", &header(b"mdhd", 48_000, 0)));
        let mut file = ftyp(b"isom");
        file.extend(moov(
            1_000,
            &[
                atom(b"trak", &nameless),
                trak(
                    b"soun",
                    header(b"mdhd", 48_000, 0),
                    Some(elst(&[(1_000, 1_024)])),
                ),
            ],
        ));
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

    /// `stts` or `ctts` of version `version` with `(count, value)` runs.
    fn table(kind: &[u8; 4], version: u8, runs: &[(u32, i32)]) -> Vec<u8> {
        let mut payload = vec![version, 0, 0, 0];
        payload.extend((runs.len() as u32).to_be_bytes());
        for (count, value) in runs {
            payload.extend(count.to_be_bytes());
            payload.extend(value.to_be_bytes());
        }
        atom(kind, &payload)
    }

    /// A video `trak` at 30000 whose sample table is `tables`.
    fn trak_with_samples(tables: &[Vec<u8>]) -> Vec<u8> {
        let mut hdlr = vec![0; 4];
        hdlr.extend(b"vide");
        hdlr.extend([0; 13]);
        let mut mdia = header(b"mdhd", 30_000, 30_030);
        mdia.extend(full(b"hdlr", &hdlr));
        mdia.extend(atom(b"minf", &atom(b"stbl", &tables.concat())));
        let mut payload = full(b"tkhd", &[0; 80]);
        payload.extend(atom(b"mdia", &mdia));
        atom(b"trak", &payload)
    }

    fn first_composition_of(tables: &[Vec<u8>]) -> Option<i64> {
        let mut file = ftyp(b"isom");
        file.extend(moov(1_000, &[trak_with_samples(tables)]));
        edits_of(file)[0].first_composition
    }

    #[test]
    fn the_first_shown_sample_is_found() {
        let stts = table(b"stts", 0, &[(30, 1_001)]);
        // libx264's B-frames at 30000/1001: I P B b P …, the I shown at 2002.
        let ctts = table(
            b"ctts",
            0,
            &[
                (1, 2_002),
                (1, 5_005),
                (1, 2_002),
                (1, 0),
                (1, 1_001),
                (25, 2_002),
            ],
        );
        assert_eq!(first_composition_of(&[stts.clone(), ctts]), Some(2_002));
        // Without `ctts`, the first sample at 0.
        assert_eq!(first_composition_of(&[stts]), Some(0));
        // Version 1's negative offsets.
        let stts = table(b"stts", 0, &[(2, 1_001)]);
        let ctts = table(b"ctts", 1, &[(1, 1_001), (1, -1_001)]);
        assert_eq!(first_composition_of(&[stts, ctts]), Some(0));
    }

    #[test]
    fn a_track_without_samples_has_no_first_shown_one() {
        // A fragmented file's `stts` is empty.
        assert_eq!(first_composition_of(&[table(b"stts", 0, &[])]), None);
        // An `stts` cut short is ignored.
        let mut cut = table(b"stts", 0, &[(30, 1_001)]);
        cut.truncate(cut.len() - 4);
        cut[3] = cut.len() as u8;
        assert_eq!(first_composition_of(&[cut]), None);
        // Only the first samples count: a later one shown earlier is not.
        let stts = table(b"stts", 0, &[(100, 1_000)]);
        let mut runs = vec![(64, 100_000)];
        runs.push((36, -99_000));
        let ctts = table(b"ctts", 1, &runs);
        assert_eq!(first_composition_of(&[stts, ctts]), Some(100_000));
    }
}
