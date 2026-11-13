//! Ikon ABRA di abra.exe (Explorer, Start, taskbar, tray).
include!("icon_res.rs");

fn main() {
    println!("cargo:rerun-if-changed=icon_res.rs");
    embed_icon();
}
