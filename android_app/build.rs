fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // NDK LLD's parallel debug link competes with the emulator and workspace language servers.
        println!("cargo::rustc-link-arg-cdylib=-Wl,--threads=1");
    }
}
