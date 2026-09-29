// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{records::Transform, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Pending, World};
use std::path::PathBuf;

const CELL: u32 = 0x210;
const TRIGGER: u32 = 0x320;
const ACTOR: u32 = 0x330;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn trigger_world() -> World {
    let mut primitive = Vec::new();
    for value in [10.0_f32, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0] {
        primitive.extend_from_slice(&value.to_le_bytes());
    }
    primitive.extend_from_slice(&1_u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(Record::new(Tag(*b"ACTI"), 0x410));
    let mut trigger = Record::new(Tag(*b"REFR"), TRIGGER)
        .sub(Tag(*b"NAME"), 0x410_u32.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            Transform {
                position: [0.0; 3],
                rotation: [0.0; 3],
            }
            .to_bytes(),
        )
        .sub(Tag(*b"XPRM"), primitive);
    trigger.placement.cell = Some(CELL);
    plugin.push_top(trigger);
    let mut actor = Record::new(Tag(*b"ACHR"), ACTOR)
        .sub(Tag(*b"NAME"), 0x420_u32.to_le_bytes())
        .sub(
            Tag(*b"DATA"),
            Transform {
                position: [-20.0, 0.0, 0.0],
                rotation: [0.0; 3],
            }
            .to_bytes(),
        );
    actor.placement.cell = Some(CELL);
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x420));
    plugin.push_top(actor);
    let mut order = LoadOrder::new();
    order.add("Trigger.esm", plugin).unwrap();
    World::new(order)
}

fn event_names(world: &World) -> Vec<String> {
    world
        .pending
        .iter()
        .filter_map(|event| match event {
            Pending::Event { name, .. } if name.starts_with("OnTrigger") => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn hst_14_cell_exit_removes_occupant_and_fires_one_leave() {
    let mut world = trigger_world();
    world.update_trigger_actor_position(
        id(ACTOR),
        Some(id(CELL)),
        [-20.0, 0.0, 0.0],
        [0.0; 3],
        &mut |_, _| soe_papyrus::ObjectId(1),
    );
    world.update_trigger_actor_position(
        id(ACTOR),
        None,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        &mut |_, _| soe_papyrus::ObjectId(1),
    );

    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(0));
    assert_eq!(event_names(&world), ["OnTriggerEnter", "OnTriggerLeave"]);
    assert_eq!(
        world
            .pending
            .iter()
            .filter(|event| matches!(event,
                Pending::Event { name, .. } if name == "OnTriggerLeave"
            ))
            .count(),
        1
    );
}

#[test]
fn hst_14_installed_vanilla_trigger_counts_actor_inside() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed trigger check");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path = data.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        order
            .add(
                name,
                Plugin::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}")),
            )
            .unwrap();
    }
    let mut world = World::new(order);
    let candidate = world
        .order()
        .winners_of(Tag(*b"REFR"))
        .find_map(|(id, record)| {
            let refr = soe_esm::records::refr::Refr::from_record(record).ok()?;
            let primitive = refr.primitive?;
            matches!(primitive.kind, 1 | 2).then_some((id, refr))
        })
        .expect("installed masters contain a vanilla box or sphere trigger");
    let (trigger, refr) = candidate;
    let source = world.order().winner_ref(trigger).unwrap();
    let cell = world.order().resolve(source.file, refr.cell.unwrap());
    let center = refr.transform.unwrap().position;
    let actor = world.player();
    world.update_trigger_actor_position(actor, Some(cell), center, center, &mut |_, _| {
        soe_papyrus::ObjectId(1)
    });

    assert_eq!(world.trigger_object_count(trigger), Some(1));
}
