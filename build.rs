//! Embeds the OpenTS2 icon in the Windows executables (ts2-viewer, ts2-tool).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/opents2.rc");
    println!("cargo:rerun-if-changed=assets/icon/opents2.ico");
    #[cfg(windows)]
    embed_resource::compile("assets/icon/opents2.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("embed the OpenTS2 icon");
}
