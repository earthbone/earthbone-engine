// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-33 keyword-specific linked target cases.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule, Target};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACKAGE: u32 = 0x901;
const ACTOR_BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const KEY_A: u32 = 0x904;
const KEY_B: u32 = 0x905;
const BASE: u32 = 0x906;
const FIRST: u32 = 0x907;
const SECOND: u32 = 0x908;
const CELL: u32 = 0x909;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn target_input(record: Record, target: Target) -> Record {
    let (kind, value) = match target {
        Target::LinkedReference(value) => (3i32, value),
        _ => panic!("fixture uses a linked-reference target"),
    };
    let mut data = kind.to_le_bytes().to_vec();
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [0])
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn game(target_keyword: u32, include_matching_link: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "Find"), None);
    let template = target_input(template, Target::LinkedReference(target_keyword));
    let template = package::finish(template);
    let template = package::with_procedure(template, "Find", &[0], false);
    plugin.push_top(package::with_actions(template));

    let package_record = package::close_header(
        package::build_record(PACKAGE, "FindPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let package_record = target_input(package_record, Target::LinkedReference(target_keyword));
    plugin.push_top(package::with_actions(package::finish(package_record)));
    plugin.push_top(
        npc::build_record(
            ACTOR_BASE,
            "Finder",
            "Finder",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"KYWD"), KEY_A));
    plugin.push_top(Record::new(T(b"KYWD"), KEY_B));
    plugin.push_top(Record::new(T(b"MISC"), BASE));

    let mut actor = placed(*b"ACHR", ACTOR, ACTOR_BASE, 0.0);
    actor = actor.sub(
        T(b"XLKR"),
        [KEY_A.to_le_bytes(), FIRST.to_le_bytes()].concat(),
    );
    if include_matching_link {
        actor = actor.sub(
            T(b"XLKR"),
            [KEY_B.to_le_bytes(), SECOND.to_le_bytes()].concat(),
        );
    }
    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    for record in [
        actor,
        placed(*b"REFR", FIRST, BASE, 10.0),
        placed(*b"REFR", SECOND, BASE, 20.0),
    ] {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell));
    block.entries.push(Entry::Group(children));
    let mut interior = Group::new(GroupKind::InteriorBlock(0));
    interior.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(interior));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order
        .add("FindLinked.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

#[test]
fn ai_33_find_linked_reference_uses_the_authored_keyword_not_the_first_link() {
    let game = game(KEY_B, true);
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert_eq!(ai.procedure, Procedure::Find);
    assert_eq!(ai.found_reference, Some(id(SECOND)));
}

#[test]
fn ai_33_find_linked_reference_without_a_matching_keyword_has_no_result() {
    let game = game(KEY_B, false);
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(matches!(ai.procedure, Procedure::Unsupported(_)));
    assert_eq!(ai.found_reference, None);
}
