// Embeds the app icon (drawn in src/icon_art.rs) and version info into the Windows exe, so
// the taskbar, pinned shortcuts and Explorer show it.

include!("src/icon_art.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app.ico");
    std::fs::write(&out, ico_bytes(&[16, 20, 24, 32, 40, 48, 64, 256])).expect("write icon");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(out.to_str().unwrap())
        .set("FileDescription", "Command Center")
        .set("ProductName", "Command Center");
    if let Err(e) = res.compile() {
        // Building without the Windows SDK still works; the exe just has no embedded icon.
        println!("cargo:warning=could not embed the app icon: {e}");
    }
}
