//! Supplies the C++ runtime omitted by intel_tex_2 0.2.2's prebuilt archive path.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if cfg!(feature = "encode") && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
