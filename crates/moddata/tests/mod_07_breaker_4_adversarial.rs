// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn mod_07_real_mod_acceptance_corpus_is_available() {
    let root = std::env::var_os("SOE_MOD_CORPUS")
        .map(PathBuf::from)
        .expect("MOD-07 real-mod acceptance requires SOE_MOD_CORPUS");
    assert!(
        root.is_dir(),
        "SOE_MOD_CORPUS must name the local compatibility corpus: {}",
        root.display()
    );
}
