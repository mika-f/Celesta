use std::env;
use std::path::PathBuf;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for lib in [
            "strmiids", "mfuuid", "mfplat", "oleaut32", "shlwapi", "vfw32", "ncrypt", "crypt32",
            "gdi32",
        ] {
            println!("cargo:rustc-link-lib={lib}");
        }
        link_vcpkg_x265();
    }
}

/// With `ffmpeg[x265]` installed, the static avcodec.lib references
/// `x265_api_get_*`, but ffmpeg-sys-next's vcpkg lookup leaves x265's library
/// off the link line (unlike x264's). Link it here when it is
/// installed; an FFmpeg built without x265 needs nothing extra.
fn link_vcpkg_x265() {
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKGRS_TRIPLET");
    let Some(root) = env::var_os("VCPKG_ROOT") else {
        return;
    };
    let triplet =
        env::var("VCPKGRS_TRIPLET").unwrap_or_else(|_| "x64-windows-static-md".to_owned());
    let lib_dir = PathBuf::from(root)
        .join("installed")
        .join(triplet)
        .join("lib");
    let x265 = lib_dir.join("x265-static.lib");
    println!("cargo:rerun-if-changed={}", x265.display());
    if x265.exists() {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=x265-static");
    }
}
