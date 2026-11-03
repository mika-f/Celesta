//! Checks `CELESTA_SYSTEM_PLUGIN_DIR`, the bundled plugin folder a package
//! build may fix (such as `/usr/lib/celesta/plugins`), before it is compiled
//! in: it must be absolute, or the folder would depend on where Celesta is
//! started.

fn main() {
    println!("cargo::rerun-if-env-changed=CELESTA_SYSTEM_PLUGIN_DIR");
    let Ok(dir) = std::env::var("CELESTA_SYSTEM_PLUGIN_DIR") else {
        return;
    };
    // Judged by the target's rules, not the build machine's, so a Linux
    // package can be built from Windows and the other way around.
    let windows = std::env::var("CARGO_CFG_WINDOWS").is_ok();
    let absolute = if windows {
        let bytes = dir.as_bytes();
        (bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
            || dir.starts_with(r"\\")
    } else {
        dir.starts_with('/')
    };
    assert!(
        absolute && !dir.split(['/', '\\']).any(|part| part == ".."),
        "CELESTA_SYSTEM_PLUGIN_DIR must be an absolute path without `..`, not {dir:?}"
    );
    println!("cargo::rustc-env=CELESTA_SYSTEM_PLUGIN_DIR={dir}");
}
