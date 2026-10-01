// SPDX-License-Identifier: GPL-3.0-or-later

//! Supply the C++ runtime omitted by intel_tex_2 0.2.2's prebuilt archive linkage.

fn main() {
    #[cfg(feature = "encode")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
