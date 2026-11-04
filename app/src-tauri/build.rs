//! The version the app shows is the version of its own package: Linux has its own numbering (packaging/linux/VERSION, also read by
//! the packaging script), Windows uses the workspace version that names its installer. Nothing has to be edited by hand for a release.
fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let version = if target == "linux" {
        println!("cargo:rerun-if-changed=../../packaging/linux/VERSION");
        std::fs::read_to_string("../../packaging/linux/VERSION").expect("packaging/linux/VERSION").trim().to_string()
    } else {
        std::env::var("CARGO_PKG_VERSION").unwrap()
    };
    println!("cargo:rustc-env=MARZ_RELEASE_VERSION={version}");
    tauri_build::build()
}
