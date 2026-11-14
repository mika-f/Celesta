use std::io::Read;
use std::time::Duration;

use celesta_remote::{RemoteAssetError, open_url};

use crate::transport::{Transport, TransportError};

/// How long the command line waits for the advisory list before it carries
/// on without it.
pub const CLI_ADVISORY_TIMEOUT: Duration = Duration::from_secs(3);

/// Fetches registry files over HTTPS with `celesta-remote`'s agent, so TLS
/// trust, proxies, and the user agent match the rest of Celesta.
#[derive(Debug, Clone, Default)]
pub struct HttpTransport {
    timeout: Option<Duration>,
}

impl HttpTransport {
    /// A transport with the agent's standard limits.
    pub fn new() -> Self {
        Self::default()
    }

    /// A transport whose every phase and body stall is limited to `timeout`.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
        }
    }
}

impl Transport for HttpTransport {
    fn open(&self, url: &str) -> Result<Box<dyn Read + Send + '_>, TransportError> {
        open_url(url, self.timeout).map_err(|error| match &error {
            RemoteAssetError::Request { source, .. }
                if matches!(**source, ureq::Error::StatusCode(404)) =>
            {
                TransportError::NotFound(url.to_owned())
            }
            _ => TransportError::Failed {
                url: url.to_owned(),
                message: error.to_string(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    fn serve(status: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                    line.clear();
                }
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        format!("http://{address}/file.json")
    }

    #[test]
    fn http_maps_404_to_not_found() {
        let url = serve("404 Not Found", "");
        let error = HttpTransport::new().open(&url).err().unwrap();
        assert!(matches!(error, TransportError::NotFound(found) if found == url));
    }

    #[test]
    fn http_reads_a_body_and_maps_other_statuses_to_failed() {
        let mut body = String::new();
        HttpTransport::with_timeout(Duration::from_secs(5))
            .open(&serve("200 OK", "{}"))
            .unwrap()
            .read_to_string(&mut body)
            .unwrap();
        assert_eq!(body, "{}");
        let error = HttpTransport::new()
            .open(&serve("500 Internal Server Error", ""))
            .err()
            .unwrap();
        assert!(matches!(error, TransportError::Failed { .. }), "{error}");
    }
}
