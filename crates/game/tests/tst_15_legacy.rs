// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Legacy fixture tail cuts follow the SOEW reader and named section tables.

#[path = "common/legacy.rs"]
mod legacy;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::World;
use std::path::Path;

fn order() -> LoadOrder {
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
    order
}

#[test]
fn tst_15_legacy_tails_follow_the_versioned_reader() {
    assert_eq!(soe_game::world::SAVE_VERSION, 37);
    let original = World::new(order()).save_state();
    for version in [4, 7, 8, 9, 11, 14, 19, 20, 21, 22, 24, 25, 26, 34, 35, 36] {
        let mut bytes = original.clone();
        let tail = legacy::truncate_to_version(&mut bytes, version, |candidate| {
            World::new(order()).load_state(candidate).is_ok()
        });
        assert_eq!(bytes.len() + tail, original.len(), "version {version}");
        let mut restored = World::new(order());
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.save_state(), original, "version {version}");
        bytes.push(0);
        assert!(restored.load_state(&bytes).is_err(), "version {version}");
    }
}

#[test]
fn tst_15_actor_and_reference_sections_preserve_legacy_fixture_state() {
    let actor = GlobalFormId {
        file: 0,
        object: 0x900,
    };
    let mut world = World::new(order());
    world.actor(actor).unwrap().sneaking = true;
    world.reference(actor).unwrap().alpha = 0.75;
    let original = world.save_state();
    let hash = world.state_hash();
    for version in [7, 8, 9, 11, 14, 19, 20, 21, 22, 23] {
        let mut bytes = legacy::without_exclusive_branches(&mut world, &[actor]);
        assert_eq!(
            world.save_state(),
            original,
            "fixture preserves source world"
        );
        legacy::truncate_to_version(&mut bytes, version, |candidate| {
            World::new(order()).load_state(candidate).is_ok()
        });
        let mut restored = World::new(order());
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.state_hash(), hash, "version {version}");
        assert_eq!(restored.save_state(), original, "version {version}");
    }
}

#[test]
fn tst_15_corruption_offsets_follow_serialized_fields() {
    let actor = GlobalFormId {
        file: 0,
        object: 0x900,
    };
    let mut world = World::new(order());
    world.reference(actor).unwrap().alpha = 987_654.5;
    let before = world.save_state();
    let marker = 987_654.5_f32.to_le_bytes();
    let mut corrupt = before.clone();
    let table_at = legacy::unique_offset(&corrupt, &marker) + marker.len();
    let mut reader = soe_papyrus::vm::state::Reader::new(&corrupt[table_at..]);
    assert!(
        reader.u16().unwrap() > 0,
        "reference has registered sections"
    );
    let name_len = usize::from(reader.u8().unwrap());
    let name = std::str::from_utf8(reader.take(name_len).unwrap())
        .unwrap()
        .to_owned();
    let payload_len_at = corrupt.len() - reader.remaining();
    corrupt[payload_len_at..payload_len_at + std::mem::size_of::<u32>()]
        .copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(world
        .load_state(&corrupt)
        .unwrap_err()
        .to_string()
        .contains(&format!("section {name} at reference")));
    assert_eq!(
        world.save_state(),
        before,
        "failed load preserves the live world"
    );
}

fn check_tail_offsets(path: &Path) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).unwrap() {
            check_tail_offsets(&entry.unwrap().path());
        }
    } else if path.extension().is_some_and(|extension| extension == "rs") {
        let source = std::fs::read_to_string(path).unwrap();
        for suffix in source.split("len()").skip(1) {
            if let Some(after_minus) = suffix.trim_start().strip_prefix('-') {
                let digits = after_minus
                    .trim_start()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .count();
                assert!(
                    digits < 2,
                    "literal tail offset in {}: {suffix}",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn tst_15_save_tests_have_no_literal_tail_offsets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    check_tail_offsets(&root.join("tests"));
    check_tail_offsets(&root.join("src/world/save_tests.rs"));
}
