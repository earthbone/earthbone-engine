// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn unknown_actor_value_and_missing_default_object_are_distinct() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header = fs::read_to_string(root.join("include/RE/World.h")).unwrap();
    assert!(header.contains("std::optional<float> actor_value"));
    assert!(header.contains("std::optional<FormId> default_object"));
    assert!(header.contains("UnsupportedHook::ArrowProjectile"));
    assert!(header.contains("UnsupportedHook::LoadFormBuffer"));
}
