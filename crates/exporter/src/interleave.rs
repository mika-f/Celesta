//! Merges the encoded video and audio packets into decode order for the
//! muxer, holding video back in memory and then in a spill file while the
//! audio is not yet known.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use celesta_media::{Packet, SideData, TrackPacket};
use tempfile::NamedTempFile;

use crate::ExportError;

/// Video packets kept in memory before the rest go to the spill file.
pub(crate) const SPILL_BYTES: usize = 256 << 20;
/// Packet bytes (side data included) one [`Interleaver::take_ready`] gives
/// the muxer at most, so a staged backlog is read back and written a batch
/// at a time rather than all at once.
pub(crate) const WRITE_BATCH_BYTES: usize = 32 << 20;
/// Packets one [`Interleaver::take_ready`] gives the muxer at most: each
/// costs `writePackets` about 75 bytes of JSON header, and the header is
/// capped at 16 MiB, so a batch of tiny packets must stop well before its
/// bytes reach [`WRITE_BATCH_BYTES`].
pub(crate) const WRITE_BATCH_PACKETS: usize = 16 * 1024;
pub(crate) const VIDEO_TRACK: usize = 0;
pub(crate) const AUDIO_TRACK: usize = 1;

/// A video packet whose bytes live in the spill file.
struct Spilled {
    pts: i64,
    dts: i64,
    duration: i64,
    key: bool,
    offset: u64,
    len: usize,
    /// `(id, offset, len)` of each side data.
    additional: Vec<(u32, u64, usize)>,
}

enum Staged {
    Memory(Packet),
    Spilled(Spilled),
}

impl Staged {
    fn dts(&self) -> i64 {
        match self {
            Staged::Memory(packet) => packet.dts,
            Staged::Spilled(spilled) => spilled.dts,
        }
    }

    /// The packet's bytes, side data included.
    fn bytes(&self) -> usize {
        match self {
            Staged::Memory(packet) => packet_bytes(packet),
            Staged::Spilled(spilled) => {
                spilled.len
                    + spilled
                        .additional
                        .iter()
                        .map(|&(_, _, len)| len)
                        .sum::<usize>()
            }
        }
    }
}

struct SpillFile {
    file: NamedTempFile,
    end: u64,
}

pub(crate) struct Interleaver {
    video_timescale: u32,
    audio_timescale: u32,
    spill_dir: PathBuf,
    limit: usize,
    video: VecDeque<Staged>,
    audio: VecDeque<Packet>,
    video_finished: bool,
    audio_started: bool,
    audio_finished: bool,
    staged_bytes: usize,
    spill: Option<SpillFile>,
}

fn packet_bytes(packet: &Packet) -> usize {
    packet.bytes.len()
        + packet
            .additional
            .iter()
            .map(|s| s.bytes.len())
            .sum::<usize>()
}

impl Interleaver {
    pub(crate) fn new(video_timescale: u32, spill_dir: &Path) -> Self {
        Self {
            video_timescale,
            audio_timescale: video_timescale,
            spill_dir: spill_dir.to_path_buf(),
            limit: SPILL_BYTES,
            video: VecDeque::new(),
            audio: VecDeque::new(),
            video_finished: false,
            audio_started: false,
            audio_finished: false,
            staged_bytes: 0,
            spill: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_spill_limit(mut self, bytes: usize) -> Self {
        self.limit = bytes;
        self
    }

    pub(crate) fn set_audio_timescale(&mut self, timescale: u32) {
        self.audio_timescale = timescale;
    }

    pub(crate) fn push_video(&mut self, packets: Vec<Packet>) -> Result<(), ExportError> {
        for packet in packets {
            let size = packet_bytes(&packet);
            if self.spill.is_none() && self.staged_bytes + size > self.limit {
                let file = NamedTempFile::new_in(&self.spill_dir).map_err(stage_error)?;
                self.spill = Some(SpillFile { file, end: 0 });
            }
            if let Some(spill) = &mut self.spill {
                let staged = spill.append(&packet).map_err(stage_error)?;
                self.video.push_back(Staged::Spilled(staged));
            } else {
                self.staged_bytes += size;
                self.video.push_back(Staged::Memory(packet));
            }
        }
        Ok(())
    }

    pub(crate) fn finish_video(&mut self) {
        self.video_finished = true;
    }

    pub(crate) fn push_audio(&mut self, packets: Vec<Packet>) {
        self.audio_started = true;
        self.audio.extend(packets);
    }

    /// Also for "there is no audio".
    pub(crate) fn finish_audio(&mut self) {
        self.audio_started = true;
        self.audio_finished = true;
    }

    /// The packets that are ready, in decode order: at most `budget` bytes
    /// and [`WRITE_BATCH_PACKETS`] of them, but always one when any is ready. Call it again until it
    /// gives none. A spilled packet that cannot be read back fails it, and
    /// the packets taken before it are lost.
    pub(crate) fn take_ready(&mut self, budget: usize) -> Result<Vec<TrackPacket>, ExportError> {
        let mut ready = Vec::new();
        if !self.audio_started {
            return Ok(ready);
        }
        let mut taken = 0usize;
        loop {
            let take_video = match (self.video.front(), self.audio.front()) {
                (Some(video), Some(audio)) => {
                    let video_time = i128::from(video.dts()) * i128::from(self.audio_timescale);
                    let audio_time = i128::from(audio.dts) * i128::from(self.video_timescale);
                    video_time <= audio_time
                }
                (Some(_), None) if self.audio_finished => true,
                (None, Some(_)) if self.video_finished => false,
                _ => break,
            };
            let bytes = if take_video {
                self.video.front().map_or(0, Staged::bytes)
            } else {
                self.audio.front().map_or(0, packet_bytes)
            };
            if ready.len() >= WRITE_BATCH_PACKETS
                || (!ready.is_empty() && taken.saturating_add(bytes) > budget)
            {
                break;
            }
            taken = taken.saturating_add(bytes);
            if take_video {
                let staged = self.video.pop_front().expect("video front exists");
                let packet = match staged {
                    Staged::Memory(packet) => {
                        self.staged_bytes -= packet_bytes(&packet);
                        packet
                    }
                    Staged::Spilled(spilled) => {
                        let spill = self.spill.as_mut().expect("spilled packets have a file");
                        spill.read(&spilled).map_err(|source| ExportError::Io {
                            operation: "read staged video packets",
                            source,
                        })?
                    }
                };
                ready.push(TrackPacket {
                    track: VIDEO_TRACK,
                    packet,
                });
            } else {
                let packet = self.audio.pop_front().expect("audio front exists");
                ready.push(TrackPacket {
                    track: AUDIO_TRACK,
                    packet,
                });
            }
        }
        Ok(ready)
    }

    #[cfg(test)]
    pub(crate) fn spilled(&self) -> bool {
        self.spill.is_some()
    }

    #[cfg(test)]
    pub(crate) fn staged_bytes(&self) -> usize {
        self.staged_bytes
    }
}

fn stage_error(source: io::Error) -> ExportError {
    ExportError::Io {
        operation: "stage video packets",
        source,
    }
}

impl SpillFile {
    fn append(&mut self, packet: &Packet) -> io::Result<Spilled> {
        let file: &mut File = self.file.as_file_mut();
        file.seek(SeekFrom::Start(self.end))?;
        let offset = self.end;
        file.write_all(&packet.bytes)?;
        self.end += packet.bytes.len() as u64;
        let mut additional = Vec::with_capacity(packet.additional.len());
        for side in &packet.additional {
            file.write_all(&side.bytes)?;
            additional.push((side.id, self.end, side.bytes.len()));
            self.end += side.bytes.len() as u64;
        }
        Ok(Spilled {
            pts: packet.pts,
            dts: packet.dts,
            duration: packet.duration,
            key: packet.key,
            offset,
            len: packet.bytes.len(),
            additional,
        })
    }

    fn read(&mut self, spilled: &Spilled) -> io::Result<Packet> {
        let file: &mut File = self.file.as_file_mut();
        let mut read_at = |offset: u64, len: usize| -> io::Result<Vec<u8>> {
            let mut bytes = vec![0; len];
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        };
        let bytes = read_at(spilled.offset, spilled.len)?;
        let additional = spilled
            .additional
            .iter()
            .map(|&(id, offset, len)| {
                Ok(SideData {
                    id,
                    bytes: read_at(offset, len)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Packet {
            pts: spilled.pts,
            dts: spilled.dts,
            duration: spilled.duration,
            key: spilled.key,
            bytes,
            additional,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(dts: i64, size: usize) -> Packet {
        Packet {
            pts: dts + 7,
            dts,
            duration: 1,
            key: dts == 0,
            bytes: vec![(dts % 251) as u8; size],
            additional: Vec::new(),
        }
    }

    fn times(ready: &[TrackPacket]) -> Vec<(usize, i64)> {
        ready.iter().map(|p| (p.track, p.packet.dts)).collect()
    }

    fn interleaver(dir: &Path) -> Interleaver {
        let mut interleaver = Interleaver::new(30_000, dir);
        interleaver.set_audio_timescale(48_000);
        interleaver
    }

    #[test]
    fn video_waits_for_audio_up_to_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut il = interleaver(dir.path());
        il.push_video(vec![packet(0, 4), packet(1001, 4), packet(2002, 4)])
            .unwrap();
        il.push_audio(vec![packet(0, 4), packet(1024, 4)]);
        assert_eq!(
            times(&il.take_ready(usize::MAX).unwrap()),
            [(0, 0), (1, 0), (1, 1024)]
        );
        il.finish_audio();
        assert_eq!(
            times(&il.take_ready(usize::MAX).unwrap()),
            [(0, 1001), (0, 2002)]
        );
    }

    #[test]
    fn no_audio_releases_everything() {
        let dir = tempfile::tempdir().unwrap();
        let mut il = interleaver(dir.path());
        il.finish_audio();
        for dts in [0, 1001, 2002] {
            il.push_video(vec![packet(dts, 4)]).unwrap();
        }
        let ready = il.take_ready(usize::MAX).unwrap();
        assert_eq!(times(&ready), [(0, 0), (0, 1001), (0, 2002)]);
        assert!(ready.iter().all(|p| p.track == 0));
    }

    #[test]
    fn nothing_moves_before_the_audio_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let mut il = interleaver(dir.path());
        il.push_video(vec![packet(0, 4)]).unwrap();
        assert!(il.take_ready(usize::MAX).unwrap().is_empty());
    }

    #[test]
    fn audio_waits_for_video_too() {
        let dir = tempfile::tempdir().unwrap();
        let mut il = interleaver(dir.path());
        il.push_video(vec![packet(0, 4)]).unwrap();
        il.push_audio(vec![packet(0, 4), packet(48_000, 4)]);
        assert_eq!(times(&il.take_ready(usize::MAX).unwrap()), [(0, 0)]);
        il.finish_video();
        assert_eq!(
            times(&il.take_ready(usize::MAX).unwrap()),
            [(1, 0), (1, 48_000)]
        );
    }

    fn spilling(dir: &Path) -> (Interleaver, Vec<Packet>) {
        let mut il = interleaver(dir).with_spill_limit(1024);
        let mut pushed = Vec::new();
        for n in 0..6 {
            let mut p = packet(n * 1001, 300);
            if n == 3 {
                p.additional.push(SideData {
                    id: 1,
                    bytes: vec![9; 8],
                });
            }
            il.push_video(vec![p.clone()]).unwrap();
            pushed.push(p);
            assert_eq!(il.spilled(), n >= 3);
        }
        (il, pushed)
    }

    #[test]
    fn spilled_packets_come_back_identical_and_the_file_goes() {
        let dir = tempfile::tempdir().unwrap();
        {
            let (mut il, pushed) = spilling(dir.path());
            assert_eq!(il.staged_bytes(), 900);
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            il.finish_audio();
            let ready = il.take_ready(usize::MAX).unwrap();
            let got: Vec<Packet> = ready.into_iter().map(|p| p.packet).collect();
            assert_eq!(got, pushed);
            assert_eq!(il.staged_bytes(), 0);
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_spilled_queue_keeps_decode_order() {
        let dir = tempfile::tempdir().unwrap();
        let (mut il, _) = spilling(dir.path());
        il.finish_audio();
        let dts: Vec<i64> = il
            .take_ready(usize::MAX)
            .unwrap()
            .iter()
            .map(|p| p.packet.dts)
            .collect();
        assert!(dts.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_backlog_comes_out_in_batches_within_the_budget() {
        let dir = tempfile::tempdir().unwrap();
        {
            // 900 bytes in memory, then the rest in the spill file, with one
            // packet bigger than a batch.
            let mut il = interleaver(dir.path()).with_spill_limit(1024);
            let mut pushed = Vec::new();
            for n in 0..8 {
                let size = if n == 5 { 1_000 } else { 300 };
                let p = packet(n * 1001, size);
                il.push_video(vec![p.clone()]).unwrap();
                pushed.push(p);
            }
            assert!(il.spilled());
            il.push_audio((0..4).map(|n| packet(n * 2048, 100)).collect());
            il.finish_audio();
            il.finish_video();

            let mut batches = Vec::new();
            loop {
                let batch = il.take_ready(700).unwrap();
                if batch.is_empty() {
                    break;
                }
                batches.push(batch);
            }
            assert!(batches.len() > 2, "{} batches", batches.len());
            for batch in &batches {
                let bytes: usize = batch.iter().map(|p| packet_bytes(&p.packet)).sum();
                assert!(
                    bytes <= 700 || batch.len() == 1,
                    "{bytes} bytes in {}",
                    batch.len()
                );
            }
            let all: Vec<TrackPacket> = batches.into_iter().flatten().collect();
            let seconds = |p: &TrackPacket| {
                let timescale = if p.track == VIDEO_TRACK {
                    30_000.0
                } else {
                    48_000.0
                };
                p.packet.dts as f64 / timescale
            };
            assert!(all.windows(2).all(|w| seconds(&w[0]) <= seconds(&w[1])));
            let video: Vec<Packet> = all
                .iter()
                .filter(|p| p.track == VIDEO_TRACK)
                .map(|p| p.packet.clone())
                .collect();
            assert_eq!(video, pushed);
            assert_eq!(all.len(), 12);
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_batch_of_tiny_packets_stops_at_the_packet_cap() {
        let dir = tempfile::tempdir().unwrap();
        let mut il = interleaver(dir.path());
        let count = 20_000;
        il.push_video((0..count).map(|n| packet(n * 1001, 1)).collect())
            .unwrap();
        il.finish_audio();
        let first = il.take_ready(WRITE_BATCH_BYTES).unwrap();
        assert_eq!(first.len(), WRITE_BATCH_PACKETS);
        let rest = il.take_ready(WRITE_BATCH_BYTES).unwrap();
        assert_eq!(rest.len(), count as usize - WRITE_BATCH_PACKETS);
        assert_eq!(rest[0].packet.dts, WRITE_BATCH_PACKETS as i64 * 1001);
        assert!(il.take_ready(WRITE_BATCH_BYTES).unwrap().is_empty());
    }
}
