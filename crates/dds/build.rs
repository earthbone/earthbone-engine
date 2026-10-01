//! Native link support for image_dds's intel_tex_2 encoder.
//!
//! intel_tex_2 0.2.2's build.rs links a packaged ASTC C++ object without its runtime.

fn main() {
    if cfg!(feature = "encode") && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
