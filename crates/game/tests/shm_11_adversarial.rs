// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_types::{unsupported_hook, SettingValue};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId {
        file: soe_esm::CREATED_FILE,
        object,
    }
}

fn loaded_id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn setting_lookup_is_case_insensitive_and_preserves_value_kind() {
    let mut world = World::new(LoadOrder::new());
    world
        .settings
        .insert("bfixture".into(), soe_papyrus::Value::Bool(true));
    assert_eq!(
        world.commonlib_game_setting("BFixture"),
        Some(SettingValue::Bool(true))
    );
    assert_eq!(world.commonlib_game_setting("missing"), None);
}

#[test]
fn high_process_missing_actor_is_absent_and_unknown_hook_is_named() {
    let world = World::new(LoadOrder::new());
    assert_eq!(world.commonlib_high_process(id(1)), None);
    let hook = unsupported_hook("HighProcessData", "headtrack");
    assert_eq!(hook.type_name, "HighProcessData");
    assert_eq!(hook.hook_name, "headtrack");
    assert!(!hook.reason.is_empty());
}

#[test]
fn shm_11_present_hazard_reads_live_reference_state() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"HAZD"), 0x200));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 0x300)
            .sub(Tag(*b"NAME"), 0x200_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let mut world = World::new(order);
    let placed = world.reference(loaded_id(0x300)).unwrap();
    placed.position = [4.0, 5.0, 6.0];
    placed.enabled = false;

    let hazard = world.commonlib_hazard(loaded_id(0x300)).unwrap();
    assert_eq!(hazard.reference.0, 0x300);
    assert_eq!(hazard.base.0, 0x200);
    assert_eq!(
        [hazard.position.x, hazard.position.y, hazard.position.z],
        [4.0, 5.0, 6.0]
    );
    assert!(!hazard.enabled);
}

#[test]
fn shm_11_resolved_hit_data_projects_live_combat_result() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_bits().to_le_bytes());
    dnam[8..12].copy_from_slice(&1.1_f32.to_bits().to_le_bytes());
    dnam[76..80].copy_from_slice(&6_u32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let mut world = World::new(order);
    for actor in [loaded_id(0x900), loaded_id(0x901)] {
        world.set_actor_value(actor, "health", 100.0).unwrap();
        world.set_actor_value(actor, "onehanded", 100.0).unwrap();
    }
    world.add_item(loaded_id(0x900), loaded_id(0xA00), 1);
    world
        .actor(loaded_id(0x900))
        .unwrap()
        .equipped
        .insert(loaded_id(0xA00));

    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(0);
    let hit = world
        .commonlib_resolve_hit(
            soe_game::combat::MeleeHit {
                attacker: loaded_id(0x900),
                target: loaded_id(0x901),
                weapon: loaded_id(0xA00),
                is_bash: false,
                is_power_attack: false,
            },
            Vec::new(),
            Vec::new(),
            &mut allocate,
        )
        .unwrap();
    assert_eq!(hit.attacker.0, 0x900);
    assert_eq!(hit.target.0, 0x901);
    assert_eq!(hit.weapon.0, 0xA00);
    assert!(hit.weapon_damage > 0.0);
    assert_eq!(hit.health_damage, hit.weapon_damage - hit.armor_reduction);
    assert!(!hit.blocked);
    assert!(!hit.killed);
}
