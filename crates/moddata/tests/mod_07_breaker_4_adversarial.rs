// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn mod_07_real_mod_acceptance_corpus_is_available() {
    let Some(root) = std::env::var_os("SOE_MOD_CORPUS").map(PathBuf::from) else {
        eprintln!("SOE_MOD_CORPUS is unset, skipping real-mod acceptance corpus check");
        return;
    };
    assert!(
        root.is_dir(),
        "SOE_MOD_CORPUS must name the local compatibility corpus: {}",
        root.display()
    );
}
