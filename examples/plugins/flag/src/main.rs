//! The flag plugin's process: Celesta starts it and talks to it over stdin
//! and stdout (see `celesta_plugin::protocol`).

use celesta_plugin_flag::{FlagProvider, PLUGIN};

fn main() {
    if let Err(error) = celesta_plugin::serve(
        &mut FlagProvider::default(),
        PLUGIN,
        env!("CARGO_PKG_VERSION"),
    ) {
        eprintln!("celesta-plugin-flag: {error}");
        std::process::exit(1);
    }
}
