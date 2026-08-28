fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=strmiids");
        println!("cargo:rustc-link-lib=mfuuid");
        println!("cargo:rustc-link-lib=mfplat");
        println!("cargo:rustc-link-lib=oleaut32");
        println!("cargo:rustc-link-lib=shlwapi");
        println!("cargo:rustc-link-lib=vfw32");
        println!("cargo:rustc-link-lib=ncrypt");
        println!("cargo:rustc-link-lib=crypt32");
    }
}
