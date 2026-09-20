// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Encounter-zone level sources, first-entry locks and saved actor selection.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, World};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ZONE: u32 = 0x100;
const CELL: u32 = 0x200;
const REF: u32 = 0x300;
const BASE: u32 = 0x400;
const WORLD: u32 = 0x500;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn zone(object: u32, minimum: u8, maximum: u8, flags: u8) -> Record {
    let mut bytes = vec![0; 12];
    bytes[9] = minimum;
    bytes[10] = flags;
    bytes[11] = maximum;
    Record::new(T(b"ECZN"), object).sub(T(b"DATA"), bytes)
}

fn cell(zone: Option<u32>) -> Record {
    let mut record = Record::new(T(b"CELL"), CELL);
    record.placement.world = Some(WORLD);
    if let Some(zone) = zone {
        record = record.sub(T(b"XEZN"), zone.to_le_bytes());
    }
    record
}

fn reference(zone: Option<u32>) -> Record {
    let mut record = Record::new(T(b"REFR"), REF).sub(T(b"NAME"), BASE.to_le_bytes());
    record.placement.cell = Some(CELL);
    record.placement.world = Some(WORLD);
    if let Some(zone) = zone {
        record = record.sub(T(b"XEZN"), zone.to_le_bytes());
    }
    record
}

fn build_world(records: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CONT"), BASE));
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

fn fixture(minimum: u8, maximum: u8, flags: u8) -> World {
    build_world(vec![
        zone(ZONE, minimum, maximum, flags),
        cell(Some(ZONE)),
        reference(None),
        Record::new(T(b"WRLD"), WORLD),
    ])
}

#[test]
fn prg_04_first_entry_locks_zone_and_later_entry_preserves_level_through_save() {
    let mut world = fixture(6, 20, 0);
    world.player_level = 10;
    let pristine = world.state_hash();
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 8);
    assert_eq!(world.state_hash(), pristine);
    world.load_cell(id(CELL));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 8);
    world.player_level = 50;
    world.change_cell(None);
    world.change_cell(Some(id(CELL)));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 8);
    let saved = world.save_state();
    let mut loaded = fixture(6, 20, 0);
    loaded.player_level = world.player_level;
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.encounter_level(id(REF)).unwrap(), 8);
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(loaded.save_state(), saved);
}

#[test]
fn minimum_match_pc_maximum_and_live_scaling_follow_runtime_order() {
    for (minimum, maximum, flags, player, expected) in [
        (6, 20, 0, 2, 6),
        (6, 20, 2, 2, 2),
        (6, 20, 0, 100, 20),
        (6, 0, 0, 16, 12),
        (250, 255, 0, 1, 250),
    ] {
        let mut world = fixture(minimum, maximum, flags);
        world.player_level = player;
        assert_eq!(world.encounter_level(id(REF)).unwrap(), expected);
    }
    let mut world = fixture(6, 0, 0);
    world.player_level = 16;
    world
        .settings
        .insert("flevelscalingmult".into(), Value::Float(0.25));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 8);
    world.enter_encounter_zone(id(ZONE)).unwrap();
    world
        .settings
        .insert("flevelscalingmult".into(), Value::Float(1.0));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 8);
}

#[test]
fn reference_cell_worldspace_and_no_zone_precedence() {
    for (explicit, cell_zone, expected) in [
        (Some(ZONE), Some(ZONE + 1), 5),
        (None, Some(ZONE + 1), 10),
        (None, None, 20),
        (Some(0x1e), Some(ZONE + 1), 2),
    ] {
        let mut world = build_world(vec![
            zone(ZONE, 5, 5, 0),
            zone(ZONE + 1, 10, 10, 0),
            zone(ZONE + 2, 20, 20, 0),
            cell(cell_zone),
            reference(explicit),
            Record::new(T(b"WRLD"), WORLD).sub(T(b"XEZN"), (ZONE + 2).to_le_bytes()),
        ]);
        world.player_level = 2;
        assert_eq!(world.encounter_level(id(REF)).unwrap(), expected);
    }
    let mut world = build_world(vec![
        cell(None),
        reference(None),
        Record::new(T(b"WRLD"), WORLD),
    ]);
    world.player_level = 17;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 17);
}

#[test]
fn moved_reference_uses_current_cell() {
    let mut second = Record::new(T(b"CELL"), CELL + 1).sub(T(b"XEZN"), (ZONE + 1).to_le_bytes());
    second.placement.world = Some(WORLD);
    let mut world = build_world(vec![
        zone(ZONE, 5, 5, 0),
        zone(ZONE + 1, 20, 20, 0),
        cell(Some(ZONE)),
        second,
        reference(None),
        Record::new(T(b"WRLD"), WORLD),
    ]);
    world.reference(id(REF)).unwrap().cell = Some(id(CELL + 1));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 20);
}

#[test]
fn zero_calculation_stays_unlocked_but_receiver_level_is_at_least_one() {
    let mut world = fixture(0, 0, 0);
    assert_eq!(world.enter_encounter_zone(id(ZONE)).unwrap(), 1);
    world.player_level = 10;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 6);
    world.enter_encounter_zone(id(ZONE)).unwrap();
    world.player_level = 20;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 6);
}

#[test]
fn malformed_zone_and_scaling_reject_without_locking_or_rng_changes() {
    let mut world = fixture(6, 20, 0);
    world.player_level = 10;
    world
        .settings
        .insert("flevelscalingmult".into(), Value::Float(f32::NAN));
    let before = world.save_state();
    assert!(world.enter_encounter_zone(id(ZONE)).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.encounter_level(id(BASE)).is_err());
    let mut broken = build_world(vec![]);
    assert!(broken.enter_encounter_zone(id(ZONE)).is_err());
}

#[test]
fn old_save_clears_zone_locks_and_corrupt_zone_extension_is_atomic() {
    let mut world = fixture(6, 20, 0);
    world.player_level = 10;
    let mut old = world.save_state();
    old.truncate(old.len() - 4);
    old[4..6].copy_from_slice(&16_u16.to_le_bytes());
    world.enter_encounter_zone(id(ZONE)).unwrap();
    let before = world.save_state();
    let hash = world.state_hash();
    let mut invalid = before.clone();
    let n = invalid.len();
    invalid[n - 2..].fill(0);
    assert!(world.load_state(&invalid).is_err());
    assert_eq!(world.state_hash(), hash);
    let mut duplicate = before.clone();
    let entry = duplicate[duplicate.len() - 8..].to_vec();
    let count = duplicate.len() - 12;
    duplicate[count..count + 4].copy_from_slice(&2_u32.to_le_bytes());
    duplicate.extend(entry);
    assert!(world.load_state(&duplicate).is_err());
    assert_eq!(world.state_hash(), hash);
    world.load_state(&old).unwrap();
    world.player_level = 20;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 14);
}

#[test]
fn winning_override_resolves_reference_link_in_own_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(reference(None));
    base.push_top(cell(None));
    base.push_top(Record::new(T(b"CONT"), BASE));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(reference(Some(0x0100_0100)));
    patch.push_top(zone(0x0100_0100, 33, 33, 0));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut world = World::new(order);
    assert_eq!(
        world.encounter_zone(id(REF)).unwrap(),
        Some(GlobalFormId {
            file: 1,
            object: ZONE
        })
    );
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 33);
    world
        .enter_encounter_zone(GlobalFormId {
            file: 1,
            object: ZONE,
        })
        .unwrap();
    let saved = world.save_state();
    world.load_state(&saved).unwrap();
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 33);
}

fn actor_fixture(minimum: u8, maximum: u8) -> World {
    let low = BASE + 1;
    let high = BASE + 2;
    let list = BASE + 3;
    let actor = REF + 1;
    let npc = |form| {
        npc::build_record(
            form,
            "Actor",
            "Actor",
            npc::Config::default(),
            npc::Stats::default(),
        )
    };
    let mut base = npc(BASE).sub(T(b"TPLT"), list.to_le_bytes());
    let data = base
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"ACBS"))
        .unwrap();
    data.data[18..20].copy_from_slice(&0x1fff_u16.to_le_bytes());
    let mut leveled = Record::new(T(b"LVLN"), list).sub(T(b"LVLF"), [0]);
    for (level, form) in [(1_u16, low), (20_u16, high)] {
        let mut bytes = vec![0; 12];
        bytes[..2].copy_from_slice(&level.to_le_bytes());
        bytes[4..8].copy_from_slice(&form.to_le_bytes());
        bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
        leveled = leveled.sub(T(b"LVLO"), bytes);
    }
    let mut placed = Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), BASE.to_le_bytes());
    placed.placement.cell = Some(CELL);
    build_world(vec![
        zone(ZONE, minimum, maximum, 0),
        cell(Some(ZONE)),
        reference(None),
        Record::new(T(b"WRLD"), WORLD),
        base,
        npc(low),
        npc(high),
        leveled,
        placed,
    ])
}

#[test]
fn actor_initialization_and_dynamic_placement_receive_zone_selection_level() {
    let mut world = actor_fixture(20, 20);
    let actor = REF + 1;
    let high = BASE + 2;
    world.load_cell(id(CELL));
    assert_eq!(
        world
            .actor_component_source(id(actor), npc::template::STATS)
            .unwrap(),
        id(high)
    );
    let dynamic = world.place_at(id(BASE), id(REF), false);
    assert_eq!(
        world
            .actor_component_source(dynamic, npc::template::STATS)
            .unwrap(),
        id(high)
    );
}

#[test]
fn prefetch_does_not_lock_until_entry_and_invalid_entry_preserves_current_cell() {
    let mut world = fixture(6, 0, 0);
    world.player_level = 10;
    world.prefetch_cell(id(CELL));
    world.player_level = 20;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 14);
    world.change_cell(Some(id(CELL)));
    world.player_level = 30;
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 14);
    let before = world.save_state();
    world.change_cell(Some(id(CELL + 1)));
    assert_eq!(world.save_state(), before);
    assert_eq!(world.loaded_cell(), Some(id(CELL)));
}

#[test]
fn cell_and_worldspace_override_links_resolve_in_winning_plugin_namespace() {
    for cell_link in [true, false] {
        let mut base = Plugin::new(FileHeader::default());
        base.push_top(reference(None));
        base.push_top(cell(None));
        base.push_top(Record::new(T(b"CONT"), BASE));
        base.push_top(Record::new(T(b"WRLD"), WORLD));
        let mut patch = Plugin::new(FileHeader {
            masters: vec!["Skyrim.esm".into()],
            ..FileHeader::default()
        });
        patch.push_top(if cell_link {
            cell(Some(0x0100_0100))
        } else {
            Record::new(T(b"WRLD"), WORLD).sub(T(b"XEZN"), 0x0100_0100_u32.to_le_bytes())
        });
        patch.push_top(zone(0x0100_0100, 33, 33, 0));
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", base).unwrap();
        order.add("Patch.esp", patch).unwrap();
        let world = World::new(order);
        assert_eq!(world.encounter_level(id(REF)).unwrap(), 33);
    }
}

#[test]
fn no_zone_identity_requires_skyrim_plugin_and_ignores_editor_id() {
    for (name, expected) in [("Skyrim.esm", 7), ("Other.esm", 25)] {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(zone(0x1e, 25, 25, 0).sub(T(b"EDID"), b"RenamedZone\0"));
        plugin.push_top(reference(Some(0x1e)));
        let mut order = LoadOrder::new();
        order.add(name, plugin).unwrap();
        let mut world = World::new(order);
        world.player_level = 7;
        assert_eq!(world.encounter_level(id(REF)).unwrap(), expected);
    }
}

#[test]
fn prefetched_actor_retains_existing_choice_when_entry_locks_a_later_level() {
    let mut world = actor_fixture(0, 0);
    world.player_level = 10;
    world.prefetch_cell(id(CELL));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 6);
    world.player_level = 40;
    world.change_cell(Some(id(CELL)));
    assert_eq!(world.encounter_level(id(REF)).unwrap(), 24);
    assert_eq!(
        world
            .actor_component_source(id(REF + 1), npc::template::STATS)
            .unwrap(),
        id(BASE + 1)
    );
}
