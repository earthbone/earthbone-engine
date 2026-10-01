// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-14 trigger occupancy and events from placed XPRM primitives.

use std::path::PathBuf;

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Pending, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x210;
const TRIGGER: u32 = 0x320;
const ACTOR: u32 = 0x330;
const ACTOR_BASE: u32 = 0x420;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn xprm(bounds: [f32; 3], kind: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for value in bounds.into_iter().chain([0.0_f32; 4]) {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&kind.to_le_bytes());
    data
}

fn trigger_record(object: u32, position: [f32; 3], primitive: Vec<u8>) -> Record {
    let mut record = Record::new(T(b"REFR"), object)
        .sub(T(b"NAME"), 0x410_u32.to_le_bytes())
        .sub(
            T(b"DATA"),
            Transform {
                position,
                rotation: [0.0; 3],
            }
            .to_bytes(),
        )
        .sub(T(b"XPRM"), primitive);
    record.placement.cell = Some(CELL);
    record
}

fn actor_record() -> Record {
    let mut record = Record::new(T(b"ACHR"), ACTOR)
        .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
        .sub(
            T(b"DATA"),
            Transform {
                position: [-20.0, 0.0, 0.0],
                rotation: [0.0; 3],
            }
            .to_bytes(),
        );
    record.placement.cell = Some(CELL);
    record
}

fn make_world(primitive: Vec<u8>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"ACTI"), 0x410));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
    plugin.push_top(actor_record());
    plugin.push_top(trigger_record(TRIGGER, [0.0; 3], primitive));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

fn movement(world: &mut World, from: [f32; 3], to: [f32; 3]) {
    let mut next_object = 0;
    world.update_trigger_actor_position(id(ACTOR), Some(id(CELL)), from, to, &mut |_, _| {
        next_object += 1;
        soe_papyrus::ObjectId(next_object)
    });
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
fn hst_14_box_enter_leave_and_count_change_once_per_crossing() {
    let mut world = make_world(xprm([10.0, 10.0, 10.0], 1));
    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(0));

    movement(&mut world, [-20.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(1));
    let actor_arg = world
        .pending
        .iter()
        .find_map(|event| match event {
            Pending::Event { name, args, .. } if name == "OnTriggerEnter" => args.first(),
            _ => None,
        })
        .expect("enter event has its action reference argument");
    assert_eq!(world.form_arg(actor_arg), Some(id(ACTOR)));
    movement(&mut world, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert_eq!(event_names(&world), ["OnTriggerEnter"]);

    movement(&mut world, [1.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(0));
    assert_eq!(event_names(&world), ["OnTriggerEnter", "OnTriggerLeave"]);
}

#[test]
fn hst_14_sphere_and_non_trigger_primitive_are_distinguished() {
    let mut world = make_world(xprm([5.0, 5.0, 5.0], 2));
    movement(&mut world, [10.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(1));

    let mut unsupported = make_world(xprm([5.0, 5.0, 5.0], 3));
    assert_eq!(unsupported.trigger_object_count(id(TRIGGER)), None);
    movement(&mut unsupported, [10.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    assert!(event_names(&unsupported).is_empty());

    let malformed = make_world(vec![0; 31]);
    assert_eq!(malformed.trigger_object_count(id(TRIGGER)), None);
}

#[test]
fn hst_14_fast_actor_crossing_emits_enter_then_leave() {
    let mut world = make_world(xprm([10.0, 10.0, 10.0], 1));
    movement(&mut world, [-20.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    assert_eq!(event_names(&world), ["OnTriggerEnter", "OnTriggerLeave"]);
    assert_eq!(world.trigger_object_count(id(TRIGGER)), Some(0));
}

#[test]
fn hst_14_vanilla_xprm_trigger_accepts_an_actor_crossing() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the installed trigger survey");
        return;
    };
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let mut order = LoadOrder::new();
    for name in masters {
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
        .winners_of(T(b"REFR"))
        .find_map(|(id, record)| {
            let reference = soe_esm::records::refr::Refr::from_record(record).ok()?;
            let primitive = reference.primitive?;
            matches!(primitive.kind, 1 | 2).then_some((id, reference))
        })
        .expect("installed masters contain a supported vanilla XPRM trigger");
    let (trigger_id, trigger) = candidate;
    let cell = trigger.cell.expect("vanilla trigger is placed in a cell");
    let rref = world.order().winner_ref(trigger_id).unwrap();
    let cell = world.order().resolve(rref.file, cell);
    let center = trigger.transform.unwrap().position;
    let actor = world.player();
    let mut next_object = 0;
    world.update_trigger_actor_position(actor, Some(cell), center, center, &mut |_, _| {
        next_object += 1;
        soe_papyrus::ObjectId(next_object)
    });
    assert_eq!(world.trigger_object_count(trigger_id), Some(1));
}
