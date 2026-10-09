// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::plugin::{flags, FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, LoadOrder, Tag};
use soe_game::world::World;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

#[test]
fn high_process_projects_runtime_ids_from_a_regular_plugin() {
    let mut order = LoadOrder::new();
    order.add("Base.esm", Plugin::new(FileHeader::default())).unwrap();

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    order.add("Actors.esp", plugin).unwrap();
    let mut world = World::new(order);

    {
        let actor = world.actor(id(1, 0x900)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(1, 0x901));
    }
    let view = world.commonlib_high_process(id(1, 0x900)).unwrap();

    assert_eq!(view.actor.0, 0x0100_0900);
    assert_eq!(view.combat_target.unwrap().0, 0x0100_0901);
    assert!(view.in_combat);
}

#[test]
fn light_plugin_hazard_keeps_packed_id_and_reads_current_reference_state() {
    let mut order = LoadOrder::new();
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag::new(b"HAZD"), 0x200));
    order.add("Base.esm", base).unwrap();

    let mut header = FileHeader::default();
    header.flags |= flags::LIGHT_MASTER;
    header.masters.push("Base.esm".to_owned());
    let mut light = Plugin::new(header);
    light.push_top(
        Record::new(Tag::new(b"PHZD"), 0x0100_0300)
            .sub(Tag::new(b"NAME"), 0x200_u32.to_le_bytes())
            .sub(
                Tag::new(b"DATA"),
                [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    light.push_top(
        Record::new(Tag::new(b"REFR"), 0x0100_0301)
            .sub(Tag::new(b"NAME"), 0x0000_0200_u32.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    order.add("Hazards.esl", light).unwrap();
    let mut world = World::new(order);

    let first = world.commonlib_hazard(id(1, 0x300)).unwrap();
    assert_eq!(first.reference.0, 0xFE00_0300);
    assert_eq!(first.base.0, 0x200);
    assert_eq!([first.position.x, first.position.y, first.position.z], [1.0, 2.0, 3.0]);

    let placed = world.reference(id(1, 0x301)).unwrap();
    placed.position = [4.0, 5.0, 6.0];
    placed.enabled = false;
    let second = world.commonlib_hazard(id(1, 0x301)).unwrap();
    assert_eq!(second.reference.0, 0xFE00_0301);
    assert_eq!([second.position.x, second.position.y, second.position.z], [4.0, 5.0, 6.0]);
    assert!(!second.enabled);
}

#[test]
fn game_setting_reads_live_override_case_insensitively() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"GMST"), 0x800)
            .zsub(Tag::new(b"EDID"), "fCombatDistance")
            .sub(Tag::new(b"DATA"), 12.0_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Settings.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.settings.insert(
        "fcombatdistance".to_owned(),
        soe_papyrus::Value::Float(96.0),
    );

    assert_eq!(
        world.commonlib_game_setting("FCOMBATDISTANCE"),
        Some(soe_commonlib_shim::world_types::SettingValue::Float(96.0))
    );
    assert_eq!(world.commonlib_game_setting("fMissingSetting"), None);
}
