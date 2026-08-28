fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for lib in [
            "strmiids",
            "mfuuid",
            "mfplat",
            "oleaut32",
            "shlwapi",
            "vfw32",
            "ncrypt",
            "crypt32",
            "gdi32",
        ] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
}