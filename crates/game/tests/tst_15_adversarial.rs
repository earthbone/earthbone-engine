// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::legacy;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::World;
use std::path::Path;

fn fixture() -> (World, GlobalFormId) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "TailFixture",
        "Tail fixture",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Tail.esm", plugin).unwrap();
    let actor = GlobalFormId {
        file: 0,
        object: 0x900,
    };
    let world = World::new(order);
    (world, actor)
}

#[test]
fn populated_world_section_uses_reader_measured_legacy_tail() {
    let (mut world, _) = fixture();
    world.player_level = 73;
    let original = world.save_state();
    assert!(legacy::world_section_growth(&original) > 0);
    let mut bytes = original.clone();
    legacy::empty_legacy_world_collections(&mut bytes, 36);
    legacy::truncate_to_version(&mut bytes, 36, |candidate| {
        fixture().0.load_state(candidate).is_ok()
    });
    let mut restored = fixture().0;
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.player_level, 1);
    let baseline = fixture().0;
    assert_eq!(restored.save_state(), baseline.save_state());
}

#[test]
fn actor_and_reference_section_stripping_preserves_live_fixture_and_round_trip() {
    let (mut world, actor) = fixture();
    world.actor(actor).unwrap().sneaking = true;
    world.reference(actor).unwrap().alpha = 0.625;
    let expected_hash = world.state_hash();
    let expected_save = world.save_state();
    let mut bytes = legacy::without_exclusive_branches(&mut world, &[actor]);
    assert_eq!(world.state_hash(), expected_hash);
    legacy::truncate_to_version(&mut bytes, 23, |candidate| {
        fixture().0.load_state(candidate).is_ok()
    });
    let mut restored = fixture().0;
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.state_hash(), expected_hash);
    assert_eq!(restored.save_state(), expected_save);
}

fn has_numeric_len_tail(source: &str) -> bool {
    let mut rest = source;
    while let Some(index) = rest.find("len()") {
        rest = &rest[index + "len()".len()..];
        let suffix = rest.trim_start();
        let Some(after_minus) = suffix.strip_prefix('-') else {
            continue;
        };
        let after_minus = after_minus.trim_start();
        let literal = after_minus
            .strip_prefix('(')
            .unwrap_or(after_minus)
            .trim_start();
        if literal.chars().take_while(char::is_ascii_digit).count() >= 2 {
            return true;
        }
    }
    false
}

fn check_source(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            check_source(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|extension| extension == "rs") {
        assert!(
            !has_numeric_len_tail(&std::fs::read_to_string(path).unwrap()),
            "literal tail offset remains in {}",
            path.display()
        );
    }
}

#[test]
fn source_audit_catches_parenthesized_literal_tail_offsets() {
    assert!(has_numeric_len_tail(
        &["let cut = bytes.", "len() - (", "42);"].concat()
    ));
    assert!(has_numeric_len_tail(
        &["let cut = bytes.", "len()\n - ", "42;"].concat()
    ));
    assert!(!has_numeric_len_tail(
        &["let cut = bytes.", "len() - ", "tail;"].concat()
    ));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    check_source(&root.join("tests"));
    check_source(&root.join("src/world/save_tests.rs"));
}
