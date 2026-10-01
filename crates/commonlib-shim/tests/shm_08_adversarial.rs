// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn missing_stack_extras_remain_absent_and_list_mutation_is_not_exposed() {
    let header = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/RE/ExtraData.h"),
    )
    .unwrap();
    assert!(header.contains("std::optional<ExtraEnchantment> enchantment() const"));
    assert!(header.contains("std::optional<ExtraHotkey> hotkey() const"));
    assert!(header.contains("std::optional<ExtraLeveledCreature> leveled_creature() const"));
    assert!(header.contains("ADD_UNSUPPORTED"));
    assert!(header.contains("SET_ENCHANTMENT_UNSUPPORTED"));
}
