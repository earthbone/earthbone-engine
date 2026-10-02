// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Find selects an eligible placed reference from its authored target input.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule, Target};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x900;
const PACKAGE: u32 = 0x901;
const ACTOR_BASE: u32 = 0x902;
const ACTOR: u32 = 0x903;
const OBJECT: u32 = 0x904;
const DECOY: u32 = 0x905;
const OBJECT_BASE: u32 = 0x906;
const DECOY_BASE: u32 = 0x907;
const CELL: u32 = 0x910;

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

fn with_target_input(record: Record, index: i8, target: Target) -> Record {
    let (kind, value) = match target {
        Target::ObjectId(value) => (1i32, value),
        Target::ObjectType(value) => (2i32, value),
        _ => panic!("fixture uses object id and type inputs"),
    };
    let mut data = kind.to_le_bytes().to_vec();
    data.extend_from_slice(&value.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    record
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [index as u8])
}

fn make_game(target: Target, search_location: Option<(Location, i32)>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::close_header(package::build_template(TEMPLATE, "Find"), None);
    let template = with_target_input(template, 0, target);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Find", &[0, 1], false);
    let template = package::with_input_name(template, 0, "Object", true);
    let template = package::with_input_name(template, 1, "Search Location", true);
    plugin.push_top(package::with_actions(template));

    let package_record = package::close_header(
        package::build_record(PACKAGE, "FindPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let mut package_record = with_target_input(package_record, 0, target);
    if let Some((location, radius)) = search_location {
        package_record = package::with_location_input(package_record, 1, location, radius);
    }
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
    plugin.push_top(Record::new(T(b"WEAP"), OBJECT_BASE));
    plugin.push_top(Record::new(T(b"ARMO"), DECOY_BASE));
    plugin.push_top(Record::new(T(b"ALCH"), 0x90C).sub(T(b"ENIT"), [0, 0, 0, 0, 2, 0, 0, 0]));
    plugin.push_top(Record::new(T(b"ALCH"), 0x90E).sub(T(b"ENIT"), [0; 8]));

    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1u8, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    let mut initially_disabled = placed(*b"REFR", 0x90A, OBJECT_BASE, 1.0);
    initially_disabled.flags |= soe_esm::plugin::flags::INITIALLY_DISABLED;
    let mut deleted = placed(*b"REFR", 0x90B, OBJECT_BASE, 3.0);
    deleted.flags |= soe_esm::plugin::flags::DELETED;
    for record in [
        placed(*b"ACHR", ACTOR, ACTOR_BASE, 0.0),
        initially_disabled,
        deleted,
        placed(*b"REFR", OBJECT, OBJECT_BASE, 12.0),
        placed(*b"REFR", 0x908, OBJECT_BASE, 12.0),
        placed(*b"REFR", 0x909, OBJECT_BASE, 48.0),
        placed(*b"REFR", DECOY, DECOY_BASE, 2.0),
        placed(*b"REFR", 0x90D, 0x90C, 6.0),
        placed(*b"REFR", 0x90F, 0x90E, 4.0),
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
fn ai_33_find_returns_nearest_matching_object_inside_authored_radius() {
    let game = make_game(
        Target::ObjectId(OBJECT_BASE),
        Some((Location::NearSelf, 35)),
    );
    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert_eq!(actor.ai.procedure, Procedure::Find);
    assert_eq!(actor.ai.found_reference, Some(id(OBJECT)));
}

#[test]
fn ai_33_find_object_type_matches_any_weapon_and_breaks_ties_by_identity() {
    let game = make_game(Target::ObjectType(11), None);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(OBJECT))
    );
}

#[test]
fn ai_33_find_distinguishes_authored_food_and_potion_types() {
    let potion = make_game(Target::ObjectType(14), None);
    assert_eq!(
        potion
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(0x90F))
    );
    let food = make_game(Target::ObjectType(15), None);
    assert_eq!(
        food.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(0x90D))
    );
}

#[test]
fn ai_33_find_rejects_object_type_values_without_a_known_match_rule() {
    let game = make_game(Target::ObjectType(99), None);
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(matches!(ai.procedure, Procedure::Unsupported(_)));
    assert_eq!(ai.found_reference, None);
}

#[test]
fn ai_33_find_rejects_an_unresolved_authored_location() {
    let game = make_game(
        Target::ObjectId(OBJECT_BASE),
        Some((Location::NearReference(0x999), 35)),
    );
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(matches!(ai.procedure, Procedure::Unsupported(_)));
    assert_eq!(ai.found_reference, None);
}

#[test]
fn ai_33_find_result_survives_save_and_unknown_sections_and_future_versions_are_checked() {
    let game = make_game(Target::ObjectId(OBJECT_BASE), None);
    let bytes = game.world().save_state();
    assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), 37);
    let restored = make_game(Target::ObjectId(OBJECT_BASE), None);
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        restored
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .found_reference,
        Some(id(OBJECT))
    );

    let mut unknown_section = bytes.clone();
    let section = unknown_section
        .windows(5)
        .position(|name| name == b"AI-21")
        .unwrap();
    unknown_section[section..section + 5].copy_from_slice(b"AI-00");
    assert!(restored
        .world_mut()
        .load_state(&unknown_section)
        .unwrap_err()
        .to_string()
        .contains("unknown section AI-00 at actor"));

    let mut future = bytes;
    future[4..6].copy_from_slice(&(soe_game::world::SAVE_VERSION + 1).to_le_bytes());
    assert!(restored
        .world_mut()
        .load_state(&future)
        .unwrap_err()
        .to_string()
        .contains("world state version"));
}

#[test]
fn ai_33_find_rejects_a_negative_authored_search_radius() {
    let game = make_game(
        Target::ObjectId(OBJECT_BASE),
        Some((Location::NearSelf, -1)),
    );
    let world = game.world();
    let ai = &world.actor_ref(id(ACTOR)).unwrap().ai;
    assert!(matches!(ai.procedure, Procedure::Unsupported(_)));
    assert_eq!(ai.found_reference, None);
}
