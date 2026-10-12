// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-05 package completion, combat interruption and door unlock flags.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{self, TreeState};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;
use soe_papyrus::value::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x800;
const ACTOR: u32 = 0x801;
const BASE: u32 = 0x802;
const CURRENT: u32 = 0x810;
const HIGHER: u32 = 0x811;
const DOOR_BASE: u32 = 0x820;
const DOOR: u32 = 0x821;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn package_record(object: u32, flags: u32, interrupt_override: u8, interrupt_flags: u16) -> Record {
    let mut rec = package::build_record(object, "AI05", flags, Schedule::default());
    let mut pkdt = rec.find(T(b"PKDT")).unwrap().to_vec();
    pkdt[5] = interrupt_override;
    pkdt[8..10].copy_from_slice(&interrupt_flags.to_le_bytes());
    rec.subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"PKDT"))
        .unwrap()
        .data = pkdt;
    rec
}

fn placed(tag: [u8; 4], object: u32, base: u32) -> Record {
    Record::new(T(&tag), object)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), vec![0; 24])
}

fn fixture(current_flags: u32, override_value: u8, interrupt_flags: u16) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(package_record(HIGHER, current_flags, 0, 0));
    plugin.push_top(package_record(
        CURRENT,
        current_flags,
        override_value,
        interrupt_flags,
    ));
    plugin.push_top(
        npc::build_record(
            BASE,
            "AI05Actor",
            "AI05Actor",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), HIGHER.to_le_bytes())
        .sub(T(b"PKID"), CURRENT.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"DOOR"), DOOR_BASE));
    let actor = placed(*b"ACHR", ACTOR, BASE);
    let door = placed(*b"REFR", DOOR, DOOR_BASE);
    let cell_record = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1, 0]));
    let mut temp = Group::new(GroupKind::CellTemporary(CELL));
    for record in [actor, door] {
        let index = plugin.push_record(record);
        temp.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temp));
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
        .add("AI05.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn set_current(game: &mut Game) {
    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    actor.ai.package = Some(id(CURRENT));
    actor.ai.tree = Some(TreeState {
        owner: id(CURRENT),
        branch: 0,
        active: true,
        finished: false,
        stacked: Vec::new(),
        stacked_completed: Vec::new(),
        simultaneous: None,
    });
}

#[test]
fn ai_05_must_complete_keeps_an_unfinished_lower_priority_package() {
    let mut game = fixture(package::flags::MUST_COMPLETE, 0, 0);
    set_current(&mut game);
    assert_eq!(
        ai::select(&mut game.world_mut(), id(ACTOR))
            .unwrap()
            .package,
        id(CURRENT)
    );
}

#[test]
fn ai_05_higher_priority_package_wins_without_must_complete() {
    let mut game = fixture(0, 0, 0);
    set_current(&mut game);
    assert_eq!(
        ai::select(&mut game.world_mut(), id(ACTOR))
            .unwrap()
            .package,
        id(HIGHER)
    );
}

#[test]
fn ai_05_combat_interrupt_override_releases_must_complete_package() {
    let mut game = fixture(
        package::flags::MUST_COMPLETE,
        package::flags::INTERRUPT_OVERRIDE_COMBAT,
        0,
    );
    set_current(&mut game);
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(HIGHER));
    }
    assert_eq!(
        ai::select(&mut game.world_mut(), id(ACTOR))
            .unwrap()
            .package,
        id(HIGHER)
    );
}

#[test]
fn ai_05_observe_combat_interrupt_releases_must_complete_package() {
    let mut game = fixture(
        package::flags::MUST_COMPLETE,
        0,
        package::flags::INTERRUPT_OBSERVE_COMBAT,
    );
    set_current(&mut game);
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(HIGHER));
    }
    assert_eq!(
        ai::select(&mut game.world_mut(), id(ACTOR))
            .unwrap()
            .package,
        id(HIGHER)
    );
}

#[test]
fn ai_05_door_unlock_flags_apply_at_start_and_when_leaving_package() {
    for (flags, expect_unlock) in [
        (package::flags::UNLOCK_DOORS_AT_START, true),
        (package::flags::UNLOCK_DOORS_ON_CHANGE, true),
        (0, false),
    ] {
        let mut game = fixture(flags, 0, 0);
        let mut alloc = |_: &str, _: &str| ObjectId(1);
        let actor = id(ACTOR);
        let door = id(DOOR);
        {
            let mut world = game.world_mut();
            let door_ref = world.reference(door).unwrap();
            door_ref.locked = true;
            door_ref.owner = Some(id(BASE));
        }
        if flags & package::flags::UNLOCK_DOORS_AT_START != 0 {
            game.world_mut().evaluate_package(actor, &mut alloc);
        } else {
            set_current(&mut game);
            game.world_mut().evaluate_package(actor, &mut alloc);
        }
        assert_eq!(
            game.world_mut().reference(door).unwrap().locked,
            !expect_unlock,
            "PKDT flags {flags:#x}"
        );
    }
}

#[test]
fn ai_05_unlock_flags_leave_unowned_cell_doors_locked() {
    for flag in [
        package::flags::UNLOCK_DOORS_AT_START,
        package::flags::UNLOCK_DOORS_ON_CHANGE,
    ] {
        let mut game = fixture(flag, 0, 0);
        let actor = id(ACTOR);
        let door = id(DOOR);
        game.world_mut().reference(door).unwrap().locked = true;
        let mut alloc = |_: &str, _: &str| ObjectId(1);
        if flag == package::flags::UNLOCK_DOORS_ON_CHANGE {
            set_current(&mut game);
        }

        game.world_mut().evaluate_package(actor, &mut alloc);

        assert!(
            game.world_mut().reference(door).unwrap().locked,
            "unowned door unlocked for PKDT flag {flag:#x}"
        );
    }
}
