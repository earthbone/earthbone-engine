//! Supplies the Linux C++ runtime omitted by intel_tex_2 0.2.2's build.rs.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(feature = "encode")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
}
