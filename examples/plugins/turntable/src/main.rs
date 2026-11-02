//! The turntable plugin's process: Celesta starts it and talks to it over
//! stdin and stdout (see `celesta_plugin::protocol`).

use celesta_plugin_turntable::{PLUGIN, TurntableProvider};

fn main() {
    if let Err(error) = celesta_plugin::serve(
        &mut TurntableProvider::default(),
        PLUGIN,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("celesta-plugin-turntable: {error}");
        std::process::exit(1);
    }
}
