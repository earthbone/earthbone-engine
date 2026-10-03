// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-19: scene timer actions start at their phase and complete at expiry.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::quest;
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn timed_scene() -> Record {
    Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 2u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Pause")
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 12u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 2.0f32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes())
}

fn fixture_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(timed_scene());
    plugin.push_top(quest::build_record(0x810, "SceneQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Dlg19.esp", plugin).unwrap();
    Game::new(order)
}

fn package_scene() -> Game {
    const QUEST: u32 = 0x810;
    const PRIOR: u32 = 0x811;
    const ASSIGNED: u32 = 0x812;
    const BASE: u32 = 0x813;
    const CELL: u32 = 0x814;
    const ACTOR: u32 = 0x815;
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(package::build_record(
        PRIOR,
        "PriorPackage",
        0,
        Schedule::default(),
    ));
    plugin.push_top(package::build_record(
        ASSIGNED,
        "ScenePackage",
        0,
        Schedule::default(),
    ));
    plugin.push_top(npc::build_record(
        BASE,
        "ActorBase",
        "Actor",
        Config::default(),
        Stats::default(),
    ));
    let quest_record = quest::with_alias_packages(
        quest::build_record(QUEST, "SceneQuest", 0),
        0,
        "SceneActor",
        Some(ACTOR),
        &[PRIOR],
    );
    plugin.push_top(quest_record);
    let scene = Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 1u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Assign package")
        .sub(Tag(*b"ALID"), 0i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 1u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"PNAM"), ASSIGNED.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), QUEST.to_le_bytes());
    plugin.push_top(scene);
    let mut cell = Group::new(GroupKind::CellTemporary(CELL));
    cell.entries.push(Entry::Record(
        plugin.push_record(
            Record::new(Tag(*b"ACHR"), ACTOR)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), [0u8; 24]),
        ),
    ));
    let cell_record =
        plugin.push_record(Record::new(Tag(*b"CELL"), CELL).sub(Tag(*b"DATA"), [1u8, 0]));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(cell));
    let mut subblock = Group::new(GroupKind::InteriorSubBlock(0));
    subblock.entries.push(Entry::Record(cell_record));
    subblock.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(subblock));
    let mut top = Group::new(GroupKind::Top(Tag(*b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order.add("Dlg19Package.esp", plugin).unwrap();
    Game::new(order)
}

/// dlg_19_scene_timer_action_completes_after_its_duration
#[test]
fn dlg_19_scene_timer_action_completes_after_its_duration() {
    let mut game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));

    game.tick(1.0);
    assert_eq!(game.world().scene_phase(id(0x800)), Some(0));
    assert!(!game.world_mut().complete_scene_action(id(0x800), 0));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(!game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));

    game.tick(1.0);
    game.tick(1.0);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(!game.world_mut().complete_scene_action(id(0x800), 0));
    assert!(game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    assert_eq!(game.world().scene_phase(id(0x800)), Some(1));
}

/// dlg_19_scene_timer_deadline_survives_save_load
#[test]
fn dlg_19_scene_timer_deadline_survives_save_load() {
    let mut game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    game.tick(0.75);
    let save = game.save_state().unwrap();

    let mut restored = fixture_game();
    restored.load_state(&save).unwrap();
    restored.tick(1.0);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(!restored
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
    restored.tick(0.25);
    restored.tick(0.75);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(restored
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
}

/// dlg_19_scene_package_action_restores_the_actor_package
#[test]
fn dlg_19_scene_package_action_restores_the_actor_package() {
    let mut game = package_scene();
    let actor = id(0x815);
    let quest = id(0x810);
    let scene = id(0x800);
    assert!(game.start_quest(quest));
    game.world_mut().drive_actor(actor);
    game.world_mut().force_alias(quest, 0, Some(actor));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(0x811)));

    let mut allocate = |_: &str, _: &str| ObjectId(1);
    let scene_record = game.world_mut().record(scene).unwrap().clone();
    let parsed = soe_esm::records::gameplay::Scene::from_record(&scene_record).unwrap();
    assert_eq!(parsed.phases.len(), 2, "{parsed:?}");
    assert_eq!(parsed.actions[0].start_phase, Some(0), "{parsed:?}");
    assert_eq!(parsed.actions[0].end_phase, Some(0), "{parsed:?}");
    assert_eq!(game.world().scene_phase(scene), None);
    assert!(!game.world().scenes.contains(&scene));
    assert!(game.world_mut().start_scene(scene, &mut allocate));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(0x812)));
    assert_eq!(
        game.world().actor_ref(actor).unwrap().ai.scene_package,
        Some(id(0x812))
    );

    let save = game.save_state().unwrap();
    let mut restored = package_scene();
    restored.load_state(&save).unwrap();
    assert_eq!(restored.world().current_package(actor), Some(id(0x812)));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(restored
        .world_mut()
        .advance_scene_phase(scene, &mut allocate));
    assert_eq!(restored.world().current_package(actor), Some(id(0x811)));
    assert_eq!(
        restored.world().actor_ref(actor).unwrap().ai.alias,
        Some((quest, 0))
    );
    assert_eq!(
        restored.world().actor_ref(actor).unwrap().ai.scene_package,
        None
    );
}

#[test]
fn dlg_19_stopping_scene_restores_package_and_alias_owner() {
    let mut game = package_scene();
    let actor = id(0x815);
    let quest = id(0x810);
    let scene = id(0x800);
    assert!(game.start_quest(quest));
    game.world_mut().drive_actor(actor);
    game.world_mut().force_alias(quest, 0, Some(actor));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(0x811)));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(scene, &mut allocate));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(0x812)));

    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().stop_scene(scene, &mut allocate));
    assert_eq!(game.world().current_package(actor), Some(id(0x811)));
    assert_eq!(
        game.world().actor_ref(actor).unwrap().ai.alias,
        Some((quest, 0))
    );
    assert_eq!(
        game.world().actor_ref(actor).unwrap().ai.scene_package,
        None
    );
}

#[test]
fn dlg_19_timer_action_with_zero_duration_completes_on_first_tick() {
    let mut plugin = Plugin::new(FileHeader::default());
    let zero_timer = Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Only")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 2u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Zero pause")
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 12u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0.0f32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
        .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes());
    plugin.push_top(zero_timer);
    plugin.push_top(quest::build_record(0x810, "SceneQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Dlg19Zero.esp", plugin).unwrap();
    let mut game = Game::new(order);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut allocate));
    game.tick(0.0);
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game
        .world_mut()
        .advance_scene_phase(id(0x800), &mut allocate));
}
