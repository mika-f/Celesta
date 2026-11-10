//! The Media Foundation media plugin's process: on Windows, serves the media
//! role of the plugin protocol on stdin and stdout until stdin closes;
//! elsewhere, says it runs on Windows only and fails.

use celesta_codec_mediafoundation::PLUGIN_ID;

#[cfg(windows)]
fn main() {
    use celesta_codec_mediafoundation::MfCodec;

    if let Err(error) =
        celesta_media::protocol::serve(&mut MfCodec::new(), PLUGIN_ID, env!("CARGO_PKG_VERSION"))
    {
        eprintln!("{PLUGIN_ID}: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("{PLUGIN_ID}: this media plugin runs on Windows only");
    std::process::exit(1);
}
