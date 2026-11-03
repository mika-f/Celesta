//! The VRM plugin's process: Celesta starts it and talks to it over stdin
//! and stdout (see `celesta_plugin::protocol`).

use celesta_plugin_vrm::{PLUGIN, VrmProvider};

fn main() {
    if let Err(error) = celesta_plugin::serve(&mut VrmProvider, PLUGIN, env!("CARGO_PKG_VERSION")) {
        eprintln!("celesta-plugin-vrm: {error}");
        std::process::exit(1);
    }
}
