// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! No additional breaker assertion is supported by the named format sources.
//! Original-runtime LVSP precedence traces are still required for PRG-21.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ROOT: u32 = 0x100;
const CHILD: u32 = 0x101;
const VALID: u32 = 0x102;
const LOW: u32 = 0x200;
const HIGH: u32 = 0x201;
const MISSING: u32 = 0x999;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn list(form: u32, flags: u8, chance_none: u8, entries: &[(u16, u32, u16)]) -> Record {
    let mut record = Record::new(T(b"LVSP"), form)
        .sub(T(b"LVLD"), [chance_none])
        .sub(T(b"LVLF"), [flags])
        .sub(T(b"LLCT"), [entries.len() as u8]);
    for &(level, form, count) in entries {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&form.to_le_bytes());
        bytes[8..10].copy_from_slice(&count.to_le_bytes());
        record = record.sub(T(b"LVLO"), bytes);
    }
    record
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"SPEL"), LOW));
    plugin.push_top(Record::new(T(b"SPEL"), HIGH));
    plugin.push_top(list(ROOT, 4, 0, &[(1, CHILD, 1), (1, MISSING, 1)]));
    // A nonzero chance-none field draws before the root reaches MISSING.
    plugin.push_top(list(CHILD, 0, 1, &[(1, LOW, 1)]));
    plugin.push_top(list(VALID, 0, 0, &[(1, LOW, 1), (1, HIGH, 1)]));
    let mut order = LoadOrder::new();
    order.add("Breaker.esm", plugin).unwrap();
    let mut world = World::new(order);
    for form in [ROOT, CHILD, VALID] {
        world.leveled_list(id(form)).unwrap();
    }
    world
}

#[test]
fn failed_nested_use_all_selection_restores_saved_actor_spell_rng() {
    let mut control = world();
    let mut failed = world();
    assert!(failed.select_leveled_spells(id(ROOT), 1).is_err());

    // Lazy list reads may initialize shared list state, but the next random
    // selection must still see the restored RNG state.
    assert_eq!(
        failed.select_leveled_spells(id(VALID), 1).unwrap(),
        control.select_leveled_spells(id(VALID), 1).unwrap()
    );
}
