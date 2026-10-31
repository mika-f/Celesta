//! Remote asset support: downloads `http`/`https` asset URLs into a per-user
//! disk cache so the renderers, audio mixer, and media probes can keep reading
//! plain local files.
//!
//! A URL is downloaded once and then served from the cache on every later
//! run; the cache never revalidates, so changing a remote file needs a new
//! URL (or a cleared cache). Entries are keyed by the URL as a browser would
//! request it (see `normalized`).

use std::fs;
use std::hash::Hasher;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use celesta_composition::AssetLocation;
use data_url::DataUrl;
use fnv::FnvHasher;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};
use url::Url;

mod staged;
mod stylesheet;

pub use staged::StagedFile;
pub use stylesheet::{
    FontFaceSource, StylesheetFontFace, is_font_stylesheet, stylesheet_font_faces,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FILE_NAME_LEN: usize = 96;

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
        let path = self.entry_path(&normalized(url).ok()?).ok()?;
        path.is_file().then_some(path)
    }

    /// Returns the cached file for `url`, downloading it first if needed. A
    /// `data:` URL (from a font stylesheet) is decoded instead.
    pub fn fetch(&self, url: &str) -> Result<PathBuf, RemoteAssetError> {
        let url = &normalized(url)?;
        let path = self.entry_path(url)?;
        if path.is_file() {
            return Ok(path);
        }
        if is_data_url(url) {
            let invalid = |reason| RemoteAssetError::InvalidData {
                url: shortened(url),
                reason,
            };
            let data = DataUrl::process(url)
                .map_err(|_| invalid("it has no comma before the data"))?
                .decode_to_vec()
                .map_err(|_| invalid("its base64 data is invalid"))?;
            self.store(&shortened(url), &path, &mut data.0.as_slice())?;
        } else {
            let response =
                self.agent
                    .get(url)
                    .call()
                    .map_err(|source| RemoteAssetError::Request {
                        url: url.to_owned(),
                        source: Box::new(source),
                    })?;
            self.store(url, &path, &mut response.into_body().into_reader())?;
        }
        Ok(path)
    }

    fn entry_path(&self, url: &str) -> Result<PathBuf, RemoteAssetError> {
        let name = if is_data_url(url) {
            "asset".to_owned()
        } else {
            // The URL's own file name keeps its extension, which image
            // decoders and FFmpeg use as a format hint.
            file_name(
                scheme_rest(url).ok_or_else(|| RemoteAssetError::UnsupportedScheme {
                    url: url.to_owned(),
                })?,
            )
        };
        Ok(self
            .root
            .join(format!("{:016x}", fnv1a(url.as_bytes())))
            .join(name))
    }

    /// Writes `contents` to the entry at `path`, staged beside it and renamed
    /// into place, so a failed or concurrent write never leaves a truncated
    /// file there.
    fn store(
        &self,
        url: &str,
        path: &Path,
        contents: &mut dyn io::Read,
    ) -> Result<(), RemoteAssetError> {
        let result = (|| {
            let directory = path.parent().expect("cache entries live in a directory");
            fs::create_dir_all(directory)?;
            let mut temp = StagedFile::beside(path, ".tmp")?;
            let mut writer = BufWriter::new(temp.as_file_mut());
            io::copy(contents, &mut writer)?;
            writer.flush()?;
            drop(writer);
            match temp.publish(path, true) {
                // Another fetch of the same URL finished first.
                Err(_) if path.is_file() => Ok(()),
                result => result,
            }
        })();
        result.map_err(|source| RemoteAssetError::Io {
            url: url.to_owned(),
            source,
        })
    }
}

/// `url` as a browser's fetch would request it, so the same URL written two
/// ways also shares one cache entry. An `http(s)` URL is parsed and
/// serialized again, which lowercases the scheme and host, writes an IDN host
/// as punycode, drops a default port, resolves dot segments, reads `\` as
/// `/`, adds `/` to an empty path, and percent-encodes what a request line
/// cannot carry; its fragment, which is never sent, is dropped. Other URLs
/// are kept.
fn normalized(url: &str) -> Result<String, RemoteAssetError> {
    if !is_remote_url(url) {
        return Ok(url.to_owned());
    }
    let mut parsed = Url::parse(url).map_err(|source| RemoteAssetError::InvalidUrl {
        url: url.to_owned(),
        source,
    })?;
    parsed.set_fragment(None);
    Ok(parsed.into())
}

pub(crate) fn is_data_url(url: &str) -> bool {
    url.get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
}

/// `url` cut short for a message, since a `data:` URL can hold a whole file.
fn shortened(url: &str) -> String {
    const LIMIT: usize = 64;
    match url.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{}…", &url[..end]),
        None => url.to_owned(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteAssetError {
    #[error("`{url}` is not an http, https, or data URL")]
    UnsupportedScheme { url: String },
    #[error("`{url}` is not a valid URL: {source}")]
    InvalidUrl {
        url: String,
        source: url::ParseError,
    },
    /// `url` is shortened, since a `data:` URL can hold a whole file.
    #[error("`{url}` is not a valid data: URL: {reason}")]
    InvalidData { url: String, reason: &'static str },
    #[error("{}", request_message(.url, .source))]
    Request {
        url: String,
        source: Box<ureq::Error>,
    },
    #[error("could not cache `{url}`: {source}")]
    Io { url: String, source: io::Error },
}

/// Why downloading `url` failed, with a hint for a Google Fonts stylesheet
/// that names a family or weight it does not have.
fn request_message(url: &str, error: &ureq::Error) -> String {
    if (url.starts_with("https://fonts.googleapis.com/css?")
        || url.starts_with("https://fonts.googleapis.com/css2?"))
        && matches!(error, ureq::Error::StatusCode(400))
    {
        return format!(
            "Google Fonts could not find the family or requested weights in `{url}`; check the family name and available weights at https://fonts.google.com"
        );
    }
    format!("could not download `{url}`: {error}")
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

/// 64-bit FNV-1a over the raw bytes. `Hasher::write` is used directly rather
/// than `Hash`, which would mix in a length prefix and change every cache key.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hasher = FnvHasher::default();
    hasher.write(bytes);
    hasher.finish()
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn cache_keys_are_64_bit_fnv1a() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

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
    fn normalizes_urls_as_a_browser_fetch_does() {
        let (base, hits) = serve("200 OK", b"remote bytes");
        let directory = tempfile::tempdir().unwrap();
        let cache = cache(directory.path());
        // A raw space would not make a valid request line.
        let path = cache.fetch(&format!("{base}/fonts/My Font.ttf")).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"remote bytes");
        // The same URL written another way is the same entry.
        let other = base.replacen("http://", "HTTP://", 1);
        assert_eq!(
            cache.cached(&format!("{other}/fonts/My%20Font.ttf")),
            Some(path.clone())
        );
        assert_eq!(
            cache
                .fetch(&format!("{other}/fonts/My%20Font.ttf"))
                .unwrap(),
            path
        );
        // A fragment is never sent, so it does not make another entry.
        assert_eq!(
            cache
                .fetch(&format!("{base}/fonts/My%20Font.ttf#iefix"))
                .unwrap(),
            path
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let error = cache.fetch("https://exa mple.com/a.ttf").unwrap_err();
        assert!(
            matches!(&error, RemoteAssetError::InvalidUrl { url, .. } if url == "https://exa mple.com/a.ttf"),
            "{error}"
        );
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
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn stylesheet_and_split_font_files_work_offline_in_a_new_cache_instance() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let css = b"@font-face {font-family: 'Noto Sans JP'; src: url(japanese.woff2)} @font-face {font-family: 'Noto Sans JP'; src: url(latin.woff2)}";
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let stylesheet = line.contains("/css?");
                while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                    line.clear();
                }
                let body: &[u8] = if stylesheet { css } else { b"font bytes" };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(body).unwrap();
            }
        });
        let directory = tempfile::tempdir().unwrap();
        let url = format!("{base}/css?family=Noto+Sans+JP:400&subset=japanese");
        let location = AssetLocation::Url { url: url.clone() };
        let online = cache(directory.path());
        let stylesheet = online.fetch(&url).unwrap();
        let faces =
            stylesheet_font_faces(&fs::read_to_string(&stylesheet).unwrap(), &location).unwrap();
        assert_eq!(faces.len(), 2);
        let paths: Vec<_> = faces
            .iter()
            .map(|face| {
                let FontFaceSource::File(AssetLocation::Url { url }) = &face.source else {
                    panic!("remote face")
                };
                online.fetch(url).unwrap()
            })
            .collect();
        server.join().unwrap(); // The listener is now closed: no network remains.
        drop(online);
        let offline = cache(directory.path());
        assert_eq!(offline.fetch(&url).unwrap(), stylesheet);
        for (face, path) in faces.iter().zip(paths) {
            let FontFaceSource::File(AssetLocation::Url { url }) = &face.source else {
                panic!("remote face")
            };
            assert_eq!(offline.fetch(url).unwrap(), path);
            assert_eq!(fs::read(path).unwrap(), b"font bytes");
        }
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
    fn decodes_data_urls_into_the_cache() {
        let directory = tempfile::tempdir().unwrap();
        let cache = cache(directory.path());
        // No network: a data: URL carries its own bytes.
        for (url, bytes) in [
            ("data:font/woff2;base64,d09GMg==", &b"wOF2"[..]),
            ("data:,a%20b", &b"a b"[..]),
        ] {
            assert_eq!(cache.cached(url), None);
            let path = cache.fetch(url).unwrap();
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(cache.cached(url), Some(path));
        }
        for (url, why) in [
            ("data:font/woff2;base64,***", "base64"),
            ("data:font/woff2", "comma"),
        ] {
            let error = cache.fetch(url).unwrap_err();
            assert!(
                matches!(&error, RemoteAssetError::InvalidData { reason, .. } if reason.contains(why)),
                "{error}"
            );
        }
        let long = format!("data:font/woff2;base64,{}", "A".repeat(10_000));
        let message = cache.fetch(&format!("{long}*")).unwrap_err().to_string();
        assert!(message.len() < 200, "{message}");
    }

    #[test]
    fn google_fonts_bad_request_names_the_family_and_weights() {
        for url in [
            "https://fonts.googleapis.com/css?family=Missing+Family:400",
            "https://fonts.googleapis.com/css2?family=Missing+Family:wght@400",
        ] {
            let error = RemoteAssetError::Request {
                url: url.to_owned(),
                source: Box::new(ureq::Error::StatusCode(400)),
            };
            assert!(
                error
                    .to_string()
                    .contains("could not find the family or requested weights")
            );
        }
        let error = RemoteAssetError::Request {
            url: "https://fonts.googleapis.com/css?family=Noto+Sans+JP:400".to_owned(),
            source: Box::new(ureq::Error::StatusCode(503)),
        };
        assert!(!error.to_string().contains("could not find"));
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
