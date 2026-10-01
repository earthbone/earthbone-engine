// SPDX-License-Identifier: GPL-3.0-or-later

//! Link support for image_dds encoding. intel_tex_2 0.2.2's build.rs links
//! its prebuilt ASTC C++ archive without the C++ runtime on Linux.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if cfg!(feature = "encode") && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
