// Embed the Windows resources (exe icon + version info) on Windows targets
// only; no-op elsewhere. See assets/icon.rc.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        println!("cargo:rerun-if-changed=assets/icon.rc");
        println!("cargo:rerun-if-changed=assets/icon.ico");
        embed_resource::compile("assets/icon.rc", embed_resource::NONE);
    }
}
