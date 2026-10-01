// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn poison_count_preserves_commonlibs_unsigned_domain() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let commonlib = fs::read_to_string("/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/CommonLibSSE-NG-alandtse/include/RE/E/ExtraPoison.h").unwrap();
    let rust = fs::read_to_string(root.join("src/extra_data.rs")).unwrap();
    let game = fs::read_to_string(root.join("../game/src/world/shim_extras.rs")).unwrap();
    assert!(commonlib.contains("std::uint32_t count"));
    assert!(
        rust.contains("count: u32"),
        "shim must preserve unsigned ExtraPoison count"
    );
    assert!(
        game.contains("count: *count"),
        "game adapter must preserve the full unsigned count"
    );
}
