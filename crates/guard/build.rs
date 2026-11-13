//! Ikon ABRA di abra-guard.exe.
include!("../app/icon_res.rs");

fn main() {
    println!("cargo:rerun-if-changed=../app/icon_res.rs");
    embed_icon();
}
