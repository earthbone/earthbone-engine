// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn absent_calendar_race_and_alias_values_have_optional_results() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header = fs::read_to_string(root.join("include/RE/World.h")).unwrap();
    assert!(header.contains("virtual std::optional<FormId> race() const = 0;"));
    assert!(header.contains("std::optional<FormId> reference;"));
    assert!(header.contains(
        "struct Result { bool success; std::optional<FormId> form; std::string error; }"
    ));
    assert!(header.contains("ActivateReferenceChildren"));
}
