//! Remote asset support: downloads `http`/`https` asset URLs into a per-user
//! disk cache so the renderers, audio mixer, and media probes can keep reading
//! plain local files.
//!
//! A URL is downloaded once and then served from the cache on every later
//! run; the cache never revalidates, so changing a remote file needs a new
//! URL (or a cleared cache). Entries are keyed by the exact URL string.

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use celesta_composition::AssetLocation;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_NAME_LEN: usize = 96;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Resolves an asset location to a readable local file: a relative `File`
/// path is joined onto `asset_root`, and a `Url` is downloaded into the
/// standard cache on first use.
pub fn resolve_asset_path(
    asset_root: &Path,
    location: &AssetLocation,
) -> Result<PathBuf, RemoteAssetError> {
    match location {
        AssetLocation::File { path } => {
            let path = Path::new(path);
            Ok(if path.is_absolute() {
                path.to_owned()
            } else {
                asset_root.join(path)
            })
        }
        AssetLocation::Url { url } => RemoteAssetCache::standard().fetch(url),
    }
}

/// Whether `value` is an asset URL this crate can download.
pub fn is_remote_url(value: &str) -> bool {
    scheme_rest(value).is_some()
}

pub struct RemoteAssetCache {
    root: PathBuf,
    agent: Agent,
}

impl RemoteAssetCache {
    pub fn standard() -> Self {
        Self::new(default_cache_root())
    }

    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            agent: shared_agent().clone(),
        }
    }

    /// The cached file for `url`, without touching the network.
    pub fn cached(&self, url: &str) -> Option<PathBuf> {
        let path = self.entry_path(url).ok()?;
        path.is_file().then_some(path)
    }

    /// Returns the cached file for `url`, downloading it first if needed.
    pub fn fetch(&self, url: &str) -> Result<PathBuf, RemoteAssetError> {
        let path = self.entry_path(url)?;
        if path.is_file() {
            return Ok(path);
        }
        self.download(url, &path)?;
        Ok(path)
    }

    fn entry_path(&self, url: &str) -> Result<PathBuf, RemoteAssetError> {
        let rest = scheme_rest(url).ok_or_else(|| RemoteAssetError::UnsupportedScheme {
            url: url.to_owned(),
        })?;
        // The URL's own file name keeps its extension, which image decoders
        // and FFmpeg use as a format hint.
        Ok(self
            .root
            .join(format!("{:016x}", fnv1a(url.as_bytes())))
            .join(file_name(rest)))
    }

    fn download(&self, url: &str, path: &Path) -> Result<(), RemoteAssetError> {
        let io_error = |source| RemoteAssetError::Io {
            url: url.to_owned(),
            source,
        };
        let directory = path.parent().expect("cache entries live in a directory");
        fs::create_dir_all(directory).map_err(io_error)?;
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|source| RemoteAssetError::Request {
                url: url.to_owned(),
                source: Box::new(source),
            })?;
        // Download beside the entry and rename it into place, so a failed or
        // concurrent download never leaves a truncated file at `path`.
        let temp = directory.join(format!(
            ".download-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut writer = BufWriter::new(File::create(&temp)?);
            io::copy(&mut response.into_body().into_reader(), &mut writer)?;
            writer.flush()?;
            drop(writer);
            match fs::rename(&temp, path) {
                // Another download of the same URL finished first.
                Err(_) if path.is_file() => Ok(()),
                result => result,
            }
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(io_error)
    }
}

#[derive(Debug)]
pub enum RemoteAssetError {
    UnsupportedScheme {
        url: String,
    },
    Request {
        url: String,
        source: Box<ureq::Error>,
    },
    Io {
        url: String,
        source: io::Error,
    },
}

impl fmt::Display for RemoteAssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme { url } => {
                write!(formatter, "`{url}` is not an http or https URL")
            }
            Self::Request { url, source } => {
                write!(formatter, "could not download `{url}`: {source}")
            }
            Self::Io { url, source } => {
                write!(formatter, "could not cache `{url}`: {source}")
            }
        }
    }
}

impl Error for RemoteAssetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsupportedScheme { .. } => None,
            Self::Request { source, .. } => Some(source.as_ref()),
            Self::Io { source, .. } => Some(source),
        }
    }
}

fn shared_agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            // The OS trust store, so TLS-intercepting corporate proxies work.
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .user_agent(concat!("celesta/", env!("CARGO_PKG_VERSION")))
            .build()
            .into()
    })
}

/// The part of an `http`/`https` URL after `://`.
fn scheme_rest(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        .then_some(rest)
        .filter(|rest| !rest.is_empty())
}

/// A file-system-safe name from the URL path's last segment.
fn file_name(rest: &str) -> String {
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let segment = rest
        .split_once('/')
        .map_or("", |(_, path)| path.rsplit('/').next().unwrap_or_default());
    let name: String = segment
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_start_matches('.');
    if name.is_empty() {
        return "asset".to_owned();
    }
    // Keep the tail, which holds the extension.
    name[name.len().saturating_sub(MAX_FILE_NAME_LEN)..].to_owned()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn default_cache_root() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Caches/com.natsuneko.celesta/remote-v1");
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("Celesta/remote-v1");
    }
    if let Some(cache) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(cache).join("celesta/remote-v1");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".cache/celesta/remote-v1");
    }
    std::env::temp_dir().join("celesta-remote-cache-v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::thread;

    /// Serves `status` + `body` to every request and counts the requests.
    fn serve(status: &'static str, body: &'static [u8]) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                    line.clear();
                }
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        (format!("http://{address}"), hits)
    }

    fn cache(root: &Path) -> RemoteAssetCache {
        RemoteAssetCache {
            root: root.to_owned(),
            // No proxy: the test server is local.
            agent: Agent::config_builder().proxy(None).build().into(),
        }
    }

    #[test]
    fn downloads_once_and_then_serves_from_cache() {
        let (base, hits) = serve("200 OK", b"remote bytes");
        let directory = tempfile::tempdir().unwrap();
        let cache = cache(directory.path());
        let url = format!("{base}/images/Portrait%20A.png?v=2");

        assert_eq!(cache.cached(&url), None);
        let path = cache.fetch(&url).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"remote bytes");
        assert_eq!(path.file_name().unwrap(), "Portrait_20A.png");
        assert_eq!(cache.fetch(&url).unwrap(), path);
        assert_eq!(cache.cached(&url), Some(path));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn http_errors_leave_nothing_cached() {
        let (base, _) = serve("404 Not Found", b"missing");
        let directory = tempfile::tempdir().unwrap();
        let cache = cache(directory.path());
        let url = format!("{base}/voice.wav");

        let error = cache.fetch(&url).unwrap_err();
        assert!(matches!(error, RemoteAssetError::Request { .. }), "{error}");
        assert_eq!(cache.cached(&url), None);
        let entry = directory
            .path()
            .join(format!("{:016x}", fnv1a(url.as_bytes())));
        assert_eq!(fs::read_dir(entry).unwrap().count(), 0);
    }

    #[test]
    fn rejects_non_http_urls() {
        let directory = tempfile::tempdir().unwrap();
        let error = cache(directory.path())
            .fetch("ftp://example.com/a.png")
            .unwrap_err();
        assert!(matches!(error, RemoteAssetError::UnsupportedScheme { .. }));
        assert!(!is_remote_url("ftp://example.com/a.png"));
        assert!(!is_remote_url("images/a.png"));
        assert!(is_remote_url("HTTPS://example.com/a.png"));
    }

    #[test]
    fn resolves_file_locations_against_the_asset_root() {
        let root = Path::new("project");
        let relative = AssetLocation::File {
            path: "media/a.png".to_owned(),
        };
        assert_eq!(
            resolve_asset_path(root, &relative).unwrap(),
            root.join("media/a.png")
        );
        let absolute = std::env::temp_dir().join("a.png");
        let location = AssetLocation::File {
            path: absolute.to_string_lossy().into_owned(),
        };
        assert_eq!(resolve_asset_path(root, &location).unwrap(), absolute);
    }

    #[test]
    fn names_cache_entries_after_the_url_path() {
        assert_eq!(file_name("example.com"), "asset");
        assert_eq!(file_name("example.com/"), "asset");
        assert_eq!(file_name("example.com/a/b.mp3#t=1"), "b.mp3");
        assert_eq!(file_name("example.com/..?x=/y.png"), "asset");
        assert_eq!(file_name("example.com/.hidden.png"), "hidden.png");
        let long = format!("example.com/{}.webp", "x".repeat(200));
        let name = file_name(&long);
        assert_eq!(name.len(), MAX_FILE_NAME_LEN);
        assert!(name.ends_with(".webp"));
    }
}
