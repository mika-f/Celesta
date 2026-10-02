fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=strmiids");
        println!("cargo:rustc-link-lib=mfuuid");
        println!("cargo:rustc-link-lib=mfplat");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        embed_info_plist();
    }
}

/// An unbundled macOS executable (`cargo run`) shows its file name,
/// `celesta-editor`, in the menu bar and Dock. Embedding an Info.plist in the
/// `__TEXT,__info_plist` section gives it the app's display name instead. The
/// packaged `Celesta.app` uses its bundle's own Info.plist.
fn embed_info_plist() {
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Celesta</string>
    <key>CFBundleDisplayName</key>
    <string>Celesta</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
</dict>
</plist>
"#
    );
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let path = std::path::Path::new(&out_dir).join("Info.plist");
    std::fs::write(&path, plist).expect("write the embedded Info.plist");
    // One `-Xlinker` per operand: `-Wl,` would split a path containing a comma.
    let path = path.display().to_string();
    for operand in ["-sectcreate", "__TEXT", "__info_plist", path.as_str()] {
        println!("cargo:rustc-link-arg-bins=-Xlinker");
        println!("cargo:rustc-link-arg-bins={operand}");
    }
}
