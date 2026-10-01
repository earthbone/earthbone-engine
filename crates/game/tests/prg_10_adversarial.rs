// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::flags as record_flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ZONE_A: u32 = 0x100;
const ZONE_B: u32 = 0x101;
const CELL: u32 = 0x200;
const REF_A: u32 = 0x300;
const REF_B: u32 = 0x301;
const PERSISTENT: u32 = 0x302;
const BASE: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn zone(object: u32) -> Record {
    let mut data = vec![0; 12];
    data[9] = 5;
    data[11] = 30;
    Record::new(T(b"ECZN"), object).sub(T(b"DATA"), data)
}

fn reference(object: u32, zone: u32, persistent: bool) -> Record {
    let mut record = Record::new(T(b"REFR"), object)
        .sub(T(b"NAME"), BASE.to_le_bytes())
        .sub(T(b"XEZN"), zone.to_le_bytes());
    record.placement.cell = Some(CELL);
    if persistent {
        record.flags |= record_flags::PERSISTENT;
    }
    record
}

fn fixture() -> World {
    let base = Record::new(T(b"CONT"), BASE).sub(
        T(b"CNTO"),
        [
            0x600_u32.to_le_bytes().as_slice(),
            2_i32.to_le_bytes().as_slice(),
        ]
        .concat(),
    );
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        zone(ZONE_A),
        zone(ZONE_B),
        base,
        Record::new(T(b"MISC"), 0x600),
        reference(REF_A, ZONE_A, false),
        reference(REF_B, ZONE_B, false),
        reference(PERSISTENT, ZONE_A, true),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("ResetZones.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn prg_10_expiry_resets_only_its_zone_and_keeps_persistent_reference() {
    let mut world = fixture();
    world.player_level = 10;
    world.clock.game_days = 1.0;
    world.enter_encounter_zone(id(ZONE_A)).unwrap();
    world.enter_encounter_zone(id(ZONE_B)).unwrap();
    for reference in [REF_A, REF_B, PERSISTENT] {
        world.reference(id(reference)).unwrap().opened = true;
        world.add_item(id(reference), id(0x600), -2);
    }

    world.player_level = 20;
    world.clock.game_days = 12.0;
    assert_eq!(world.enter_encounter_zone(id(ZONE_A)).unwrap(), 14);
    assert!(!world.reference(id(REF_A)).unwrap().opened);
    assert_eq!(world.item_count(id(REF_A), id(0x600)), 2);
    assert!(world.reference(id(REF_B)).unwrap().opened);
    assert_eq!(world.item_count(id(REF_B), id(0x600)), 0);
    assert!(world.reference(id(PERSISTENT)).unwrap().opened);
    assert_eq!(world.item_count(id(PERSISTENT), id(0x600)), 0);
}

#[test]
fn prg_10_invalid_clock_does_not_mutate_reset_history_or_contents() {
    let mut world = fixture();
    world.player_level = 10;
    world.clock.game_days = 1.0;
    world.enter_encounter_zone(id(ZONE_A)).unwrap();
    world.reference(id(REF_A)).unwrap().opened = true;
    world.add_item(id(REF_A), id(0x600), -2);
    world.clock.game_days = f64::NAN;

    assert!(world.enter_encounter_zone(id(ZONE_A)).is_err());
    world.clock.game_days = 10.999;
    assert_eq!(world.enter_encounter_zone(id(ZONE_A)).unwrap(), 8);
    assert!(world.reference(id(REF_A)).unwrap().opened);
}

#[test]
fn prg_10_saved_visit_keeps_expiry_deadline_after_reload() {
    let mut world = fixture();
    world.player_level = 10;
    world.clock.game_days = 1.0;
    world.enter_encounter_zone(id(ZONE_A)).unwrap();
    world.reference(id(REF_A)).unwrap().opened = true;
    let saved = world.save_state();

    let mut loaded = fixture();
    loaded.load_state(&saved).unwrap();
    loaded.player_level = 20;
    loaded.clock.game_days = 12.0;
    assert_eq!(loaded.enter_encounter_zone(id(ZONE_A)).unwrap(), 14);
    assert!(!loaded.reference(id(REF_A)).unwrap().opened);
}
