// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial reservation and interruption checks for UseIdleMarker.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACK: u32 = 0x901;
const INTERRUPT_PACK: u32 = 0x908;
const INTERRUPT_TEMPLATE: u32 = 0x909;
const BASE: u32 = 0x902;
const BASE_A: u32 = 0x90a;
const MARKER_BASE: u32 = 0x903;
const ACTOR_A: u32 = 0x904;
const ACTOR_B: u32 = 0x905;
const MARKER_A: u32 = 0x906;
const MARKER_B: u32 = 0x907;
const CELL: u32 = 0x910;
const INTERRUPT: u32 = 0x911;
const IDLE: u32 = 0x912;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "Idle"), None);
    let template = package::with_location_input(template, 0, Location::NearSelf, 100);
    let template = package::finish(template);
    let template = package::with_procedure(template, "UseIdleMarker", &[0], true);
    plugin.push_top(package::with_actions(template));
    let pack = package::close_header(
        package::build_record(PACK, "IdlePackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    plugin.push_top(package::with_actions(package::finish(pack)));
    let interrupt_template = package::close_header(
        package::build_template(INTERRUPT_TEMPLATE, "Interrupt"),
        None,
    );
    let interrupt_template = package::finish(interrupt_template);
    let interrupt_template =
        package::with_procedure(interrupt_template, "HoldPosition", &[], false);
    plugin.push_top(package::with_actions(interrupt_template));
    let interrupt_condition = package::Condition {
        comparison: package::condition::EQUAL,
        flags: 0,
        value: package::CompValue::Float(0.0),
        function: 74,
        param1: INTERRUPT,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let interrupt_pack = package::close_header(
        package::build_record(INTERRUPT_PACK, "InterruptPackage", 0, Schedule::default()),
        Some(INTERRUPT_TEMPLATE),
    );
    let interrupt_pack = package::with_condition(interrupt_pack, &interrupt_condition);
    plugin.push_top(package::with_actions(package::finish(interrupt_pack)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "IdleActor",
            "Idle actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(
        npc::build_record(
            BASE_A,
            "InterruptibleIdleActor",
            "Interruptible idle actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), INTERRUPT_PACK.to_le_bytes())
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"IDLM"), MARKER_BASE).sub(T(b"IDLA"), IDLE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"IDLE"), IDLE).zsub(T(b"EDID"), "MarkerIdle"));
    plugin.push_top(
        Record::new(T(b"GLOB"), INTERRUPT)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );
    let cell_record = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    for record in [
        placed(*b"ACHR", ACTOR_A, BASE_A, 0.0),
        placed(*b"ACHR", ACTOR_B, BASE, 5.0),
        placed(*b"REFR", MARKER_A, MARKER_BASE, 20.0),
        placed(*b"REFR", MARKER_B, MARKER_BASE, 80.0),
    ] {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell_record));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add("Idle.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR_A)));
    assert!(game.world_mut().drive_actor(id(ACTOR_B)));
    game.tick(0.1);
    game
}

#[test]
fn ai_23_idle_marker_reservation_is_exclusive_and_released_on_package_change() {
    let mut game = game();
    let first = game
        .world()
        .actor_ref(id(ACTOR_A))
        .unwrap()
        .ai
        .procedure
        .clone();
    let second = game
        .world()
        .actor_ref(id(ACTOR_B))
        .unwrap()
        .ai
        .procedure
        .clone();
    let held = match first {
        Procedure::UseIdleMarker {
            marker: Some(marker),
        } => marker,
        other => panic!("first actor did not reserve a marker: {other:?}"),
    };
    let other = match second {
        Procedure::UseIdleMarker {
            marker: Some(marker),
        } => marker,
        other => panic!("second actor did not reserve a marker: {other:?}"),
    };
    assert_ne!(held, other, "two actors reserved the same marker");
    game.world_mut().set_global(id(INTERRUPT), 0.0);
    game.tick(1.1);
    {
        let world = game.world();
        let interrupted = world.actor_ref(id(ACTOR_A)).unwrap();
        assert_eq!(interrupted.ai.package, Some(id(INTERRUPT_PACK)));
        assert_eq!(interrupted.ai.procedure, Procedure::HoldPosition);
    }
    {
        let mut world = game.world_mut();
        let center = world.reference(held).unwrap().position;
        let ai = &mut world.actor(id(ACTOR_B)).unwrap().ai;
        ai.procedure = Procedure::UseIdleMarker { marker: None };
        ai.center = Some(center);
        ai.radius = 0.0;
    }
    game.tick(0.1);
    let selected = game
        .world()
        .actor_ref(id(ACTOR_B))
        .unwrap()
        .ai
        .procedure
        .clone();
    assert_eq!(selected, Procedure::UseIdleMarker { marker: Some(held) });
}

#[test]
fn ai_23_idle_marker_skips_unusable_references() {
    for unusable in 0..3 {
        let mut game = game();
        let marker = match game.world().actor_ref(id(ACTOR_A)).unwrap().ai.procedure {
            Procedure::UseIdleMarker {
                marker: Some(marker),
            } => marker,
            ref other => panic!("first actor did not reserve a marker: {other:?}"),
        };
        {
            let mut world = game.world_mut();
            let center = world.reference(marker).unwrap().position;
            let reference = world.reference(marker).unwrap();
            match unusable {
                0 => reference.enabled = false,
                1 => reference.deleted = true,
                _ => reference.destroyed = true,
            }
            let ai = &mut world.actor(id(ACTOR_A)).unwrap().ai;
            ai.procedure = Procedure::UseIdleMarker { marker: None };
            ai.center = Some(center);
            ai.radius = 0.0;
        }
        game.tick(0.1);
        assert_eq!(
            game.world().actor_ref(id(ACTOR_A)).unwrap().ai.procedure,
            Procedure::UseIdleMarker { marker: None },
            "unusable marker kind {unusable} was selected"
        );
    }
}

#[test]
fn ai_23_plays_the_authored_idle_once_when_the_reserved_marker_is_reached() {
    let mut game = game();
    game.tick(1.0);

    let requests: Vec<_> = game
        .world()
        .animation_requests
        .iter()
        .map(|request| (request.target, request.animation.clone()))
        .collect();
    assert_eq!(
        requests,
        [
            (id(ACTOR_A), "MarkerIdle".to_owned()),
            (id(ACTOR_B), "MarkerIdle".to_owned()),
        ]
    );

    game.tick(1.0);
    assert_eq!(game.world().animation_requests.len(), 2);
}

#[test]
fn ai_23_installed_idle_markers_have_authored_idle_links() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let bytes = std::fs::read(format!("{data}/Skyrim.esm")).unwrap();
    let mut load_order = LoadOrder::new();
    load_order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();

    let markers: Vec<_> = load_order.winners_of(T(b"IDLM")).collect();
    assert!(!markers.is_empty(), "Skyrim.esm contains IDLM records");
    let with_idle_links = markers
        .iter()
        .filter(|(_, record)| record.find(T(b"IDLA")).is_some())
        .count();
    assert!(
        with_idle_links > 0,
        "vanilla IDLM records author IDLA links"
    );
}
