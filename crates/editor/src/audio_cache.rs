use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use mikan_media::AudioBuffer;

const MAGIC: &[u8; 8] = b"MIKANPCM";
const VERSION: u32 = 1;
const HEADER_LEN: u64 = 32;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct CachedAudio {
    pub(crate) buffer: AudioBuffer,
    pub(crate) waveform: Vec<f32>,
}

pub(crate) struct DiskAudioCache {
    root: PathBuf,
}

impl DiskAudioCache {
    pub(crate) fn standard() -> Self {
        Self::new(default_cache_root())
    }

    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn load(
        &self,
        source: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> io::Result<Option<CachedAudio>> {
        let path = self.entry_path(source, sample_rate, channels)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let cached = decode(file)?;
        if cached.buffer.sample_rate != sample_rate || cached.buffer.channels != channels {
            return Err(invalid_cache("cache format does not match its key"));
        }
        Ok(Some(cached))
    }

    pub(crate) fn store(
        &self,
        source: &Path,
        buffer: &AudioBuffer,
        waveform: &[f32],
    ) -> io::Result<()> {
        let path = self.entry_path(source, buffer.sample_rate, buffer.channels)?;
        fs::create_dir_all(&self.root)?;
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = self.root.join(format!(
            ".{}.{}.{}.tmp",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio"),
            std::process::id(),
            sequence
        ));
        let result = (|| {
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            let mut writer = BufWriter::new(file);
            encode(&mut writer, buffer, waveform)?;
            let file = writer.into_inner()?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn entry_path(&self, source: &Path, sample_rate: u32, channels: u16) -> io::Result<PathBuf> {
        let canonical = fs::canonicalize(source)?;
        let metadata = fs::metadata(&canonical)?;
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "mtime predates Unix epoch"))?;
        let mut hasher = DefaultHasher::new();
        canonical.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        #[cfg(unix)]
        {
            metadata.dev().hash(&mut hasher);
            metadata.ino().hash(&mut hasher);
        }
        modified.as_secs().hash(&mut hasher);
        modified.subsec_nanos().hash(&mut hasher);
        sample_rate.hash(&mut hasher);
        channels.hash(&mut hasher);
        Ok(self.root.join(format!("{:016x}.pcm", hasher.finish())))
    }
}

fn default_cache_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Caches/com.natsuneko.mikan/audio-v1");
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("Mikan/audio-v1");
    }
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(cache).join("mikan/audio-v1");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache/mikan/audio-v1");
    }
    std::env::temp_dir().join("mikan-audio-cache-v1")
}

fn encode(writer: &mut impl Write, buffer: &AudioBuffer, waveform: &[f32]) -> io::Result<()> {
    let sample_count = u64::try_from(buffer.samples.len())
        .map_err(|_| invalid_cache("sample count is too large"))?;
    let waveform_count =
        u32::try_from(waveform.len()).map_err(|_| invalid_cache("waveform count is too large"))?;
    writer.write_all(MAGIC)?;
    writer.write_all(&VERSION.to_le_bytes())?;
    writer.write_all(&buffer.sample_rate.to_le_bytes())?;
    writer.write_all(&buffer.channels.to_le_bytes())?;
    writer.write_all(&0_u16.to_le_bytes())?;
    writer.write_all(&sample_count.to_le_bytes())?;
    writer.write_all(&waveform_count.to_le_bytes())?;
    for sample in &buffer.samples {
        writer.write_all(&sample.to_le_bytes())?;
    }
    for peak in waveform {
        writer.write_all(&peak.to_le_bytes())?;
    }
    Ok(())
}

fn decode(file: File) -> io::Result<CachedAudio> {
    let file_len = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let mut magic = [0_u8; 8];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid_cache("invalid magic"));
    }
    let version = read_u32(&mut reader)?;
    if version != VERSION {
        return Err(invalid_cache("unsupported version"));
    }
    let sample_rate = read_u32(&mut reader)?;
    let channels = read_u16(&mut reader)?;
    let _reserved = read_u16(&mut reader)?;
    let sample_count = read_u64(&mut reader)?;
    let waveform_count = read_u32(&mut reader)?;
    if sample_rate == 0 || channels == 0 || sample_count % u64::from(channels) != 0 {
        return Err(invalid_cache("invalid audio format"));
    }
    let value_count = sample_count
        .checked_add(u64::from(waveform_count))
        .ok_or_else(|| invalid_cache("cache size overflow"))?;
    let expected_len = HEADER_LEN
        .checked_add(
            value_count
                .checked_mul(4)
                .ok_or_else(|| invalid_cache("cache size overflow"))?,
        )
        .ok_or_else(|| invalid_cache("cache size overflow"))?;
    if file_len != expected_len {
        return Err(invalid_cache("cache length mismatch"));
    }
    let mut samples = Vec::with_capacity(
        usize::try_from(sample_count).map_err(|_| invalid_cache("sample count is too large"))?,
    );
    for _ in 0..sample_count {
        samples.push(read_f32(&mut reader)?);
    }
    let mut waveform = Vec::with_capacity(
        usize::try_from(waveform_count)
            .map_err(|_| invalid_cache("waveform count is too large"))?,
    );
    for _ in 0..waveform_count {
        waveform.push(read_f32(&mut reader)?);
    }
    if samples
        .iter()
        .chain(&waveform)
        .any(|value| !value.is_finite())
    {
        return Err(invalid_cache("cache contains non-finite values"));
    }
    Ok(CachedAudio {
        buffer: AudioBuffer {
            sample_rate,
            channels,
            samples,
        },
        waveform,
    })
}

fn read_u16(reader: &mut impl Read) -> io::Result<u16> {
    let mut bytes = [0_u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> io::Result<u64> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_f32(reader: &mut impl Read) -> io::Result<f32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(f32::from_le_bytes(bytes))
}

fn invalid_cache(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mikan-audio-cache-{label}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn round_trips_and_invalidates_cached_audio_with_source_identity() {
        let root = fixture_root("round-trip");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("voice.wav");
        fs::write(&source, b"first").unwrap();
        let cache = DiskAudioCache::new(root.join("cache"));
        let buffer = AudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![0.25, -0.25, 0.5, -0.5],
        };
        let waveform = vec![0.25, 0.5];

        cache.store(&source, &buffer, &waveform).unwrap();
        let loaded = cache.load(&source, 48_000, 2).unwrap().unwrap();
        assert_eq!(loaded.buffer, buffer);
        assert_eq!(loaded.waveform, waveform);

        fs::write(&source, b"second version").unwrap();
        assert!(cache.load(&source, 48_000, 2).unwrap().is_none());
        assert!(cache.load(&source, 44_100, 2).unwrap().is_none());
        assert!(cache.load(&source, 48_000, 1).unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_truncated_cache_entries() {
        let root = fixture_root("truncated");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("voice.wav");
        fs::write(&source, b"source").unwrap();
        let cache = DiskAudioCache::new(root.join("cache"));
        let entry = cache.entry_path(&source, 48_000, 2).unwrap();
        fs::create_dir_all(entry.parent().unwrap()).unwrap();
        fs::write(&entry, MAGIC).unwrap();

        assert!(cache.load(&source, 48_000, 2).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
