// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn all_five_reference_extra_views_are_bound_to_the_game_world() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rust = fs::read_to_string(root.join("../game/src/world/shim_extras.rs")).unwrap();
    for name in [
        "ExtraLeveledItemBase",
        "ExtraLightData",
        "LinkedRefChild",
        "ExtraMapMarker",
        "MissingLinkedRefId",
    ] {
        assert!(rust.contains(name), "missing live game binding for {name}");
    }
}
