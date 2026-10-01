// SPDX-License-Identifier: GPL-3.0-or-later

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // CONTEXT: intel_tex_2 0.2.2 links prebuilt C++ objects without their runtime.
    if cfg!(feature = "encode") && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
