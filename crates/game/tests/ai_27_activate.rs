// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Activate reaches its assigned reference and uses shared activation once.

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const TARGET_BASE: u32 = 0x904;
const TARGET: u32 = 0x905;
const DECOY: u32 = 0x906;
const CELL: u32 = 0x910;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&100.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn make_game(include_target: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut template =
        package::close_header(package::build_template(TEMPLATE, "ActivateTemplate"), None);
    let mut target_input = 0i32.to_le_bytes().to_vec();
    target_input.extend_from_slice(&TARGET.to_le_bytes());
    target_input.extend_from_slice(&0i32.to_le_bytes());
    template = template
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), target_input)
        .sub(T(b"UNAM"), [0]);
    template = package::finish(template);
    plugin.push_top(package::with_actions(package::with_procedure(
        template,
        "Activate",
        &[0],
        false,
    )));
    let package_record = package::close_header(
        package::build_record(PACK, "ActivatePackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Activator",
            "Activator",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"DOOR"), TARGET_BASE));
    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(
        plugin.push_record(placed(*b"ACHR", ACTOR, BASE, 40.0)),
    ));
    if include_target {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(placed(
                *b"REFR",
                TARGET,
                TARGET_BASE,
                80.0,
            ))));
    }
    temporary
        .entries
        .push(Entry::Record(plugin.push_record(placed(
            *b"REFR",
            DECOY,
            TARGET_BASE,
            140.0,
        ))));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell));
    block.entries.push(Entry::Group(children));
    let mut interior_block = Group::new(GroupKind::InteriorBlock(0));
    interior_block.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(interior_block));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order
        .add("Activate.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

#[test]
fn ai_27_activate_reaches_and_activates_its_resolved_target_once() {
    let mut game = make_game(true);
    for _ in 0..12 {
        game.tick(0.25);
    }
    let mut world = game.world_mut();
    let ai = world.actor_ref(id(ACTOR)).unwrap().ai.clone();
    let actor_position = world.reference(id(ACTOR)).unwrap().position;
    let target_position = world.reference(id(TARGET)).unwrap().position;
    assert_eq!(ai.procedure, Procedure::Activate);
    assert!(ai.arrived);
    assert!(ai.activate_started);
    assert!((actor_position[0] - target_position[0]).abs() <= 4.0);
    assert!(world.reference(id(TARGET)).unwrap().opened);
    assert!(!world.reference(id(DECOY)).unwrap().opened);
    drop(world);

    let bytes = game.world().save_state();
    let mut restored = make_game(true);
    restored.world_mut().load_state(&bytes).unwrap();
    for _ in 0..8 {
        restored.tick(0.25);
    }
    let mut world = restored.world_mut();
    assert!(world.reference(id(TARGET)).unwrap().opened);
    assert!(!world.reference(id(DECOY)).unwrap().opened);
    assert!(world.actor_ref(id(ACTOR)).unwrap().ai.activate_started);
}

#[test]
fn ai_27_activate_rejects_a_missing_assigned_reference() {
    let mut game = make_game(false);
    for _ in 0..3 {
        game.tick(0.1);
    }
    assert!(matches!(
        game.world_mut().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Unsupported(_)
    ));
}

#[test]
fn ai_27_activate_saves_attempt_state_and_rejects_old_and_future_versions() {
    let mut game = make_game(true);
    for _ in 0..12 {
        game.tick(0.25);
    }
    let bytes = game.world().save_state();
    let restored = make_game(true);
    restored.world_mut().load_state(&bytes).unwrap();
    assert!(
        restored
            .world_mut()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .activate_started
    );

    let mut old = bytes.clone();
    old[4..6].copy_from_slice(&(soe_game::world::SAVE_VERSION - 1).to_le_bytes());
    assert!(restored
        .world_mut()
        .load_state(&old)
        .unwrap_err()
        .to_string()
        .contains("Activate procedure requires world version 41"));
    let mut future = bytes;
    future[4..6].copy_from_slice(&(soe_game::world::SAVE_VERSION + 1).to_le_bytes());
    assert!(restored
        .world_mut()
        .load_state(&future)
        .unwrap_err()
        .to_string()
        .contains("world state version"));
}
