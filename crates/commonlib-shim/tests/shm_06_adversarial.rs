// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn rejected_save_load_mutations_preserve_the_prior_payload_contract() {
    let header = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/RE/World.h")).unwrap();
    assert!(header.contains("NamedUnsupportedHook get_change() const"));
    assert!(header.contains("NamedUnsupportedHook save_file() const"));
    assert!(header.contains("bool load_state(const std::vector<std::uint8_t>& bytes, std::string& error)"));
    assert!(header.contains("NamedUnsupportedHook load_file() const"));
}
