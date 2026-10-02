// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-33 Find target identity edge cases.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule, Target};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACKAGE: u32 = 0x901;
const ACTOR_BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const CELL: u32 = 0x910;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn target_input(record: Record, target: Target) -> Record {
    let (kind, value) = match target {
        Target::Reference(value) => (0i32, value),
        Target::Itself => (6i32, 0),
        _ => panic!("fixture uses Specific Reference or Self"),
    };
    let mut data = kind.to_le_bytes().to_vec();
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [0])
}

fn make_game(target: Target) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "Find"), None);
    let template = target_input(template, target);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Find", &[0], false);
    let template = package::with_input_name(template, 0, "Object", true);
    plugin.push_top(package::with_actions(template));

    let package_record = package::close_header(
        package::build_record(PACKAGE, "FindPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let package_record = target_input(package_record, target);
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

    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    let mut data = 0f32.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    temporary.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(T(b"ACHR"), ACTOR)
                .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
                .sub(T(b"DATA"), data),
        ),
    ));
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
        .add("Find.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

#[test]
fn ai_33_find_self_target_returns_the_actor_reference() {
    let game = make_game(Target::Itself);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(ACTOR))
    );
}

#[test]
fn ai_33_find_specific_reference_can_select_the_actor_reference() {
    let game = make_game(Target::Reference(ACTOR));
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(ACTOR))
    );
}
