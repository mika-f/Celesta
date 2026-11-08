//! The test codec's process: serves the codec protocol on stdin and stdout
//! until stdin closes, for tests whose child processes read and write media.

use celesta_codec_test::{PLUGIN_ID, TestCodec};

fn main() {
    if let Err(error) =
        celesta_media::protocol::serve(&mut TestCodec::new(), PLUGIN_ID, env!("CARGO_PKG_VERSION"))
    {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}
