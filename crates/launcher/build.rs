//! Embeds the OpenTS2 icon (the Pizza Planet token) in opents2.exe.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/opents2.rc");
    println!("cargo:rerun-if-changed=../../assets/icon/opents2.ico");
    #[cfg(windows)]
    embed_resource::compile("../../assets/icon/opents2.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("embed the OpenTS2 icon");
}
