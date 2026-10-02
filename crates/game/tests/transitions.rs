// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Walking through a load door: the player changes cell, the cell it left
//! unloads, the cell it entered loads, and nothing that mattered is lost.
//!
//! The unload rule under test is the one that makes this safe without a
//! save's change forms: a reference is dropped only when rebuilding it from
//! its record reproduces exactly what it held. Every assertion about what
//! survives is an assertion about that rule.

use soe_esm::records::Transform;
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, plugin::flags};
use soe_esm::{Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const DOOR_BASE: u32 = 0x800;
const STATIC_BASE: u32 = 0x801;
const PLAYER_BASE: u32 = 0x802;
const CELL_A: u32 = 0x810;
const CELL_B: u32 = 0x811;
const DOOR_A: u32 = 0x820;
const DOOR_B: u32 = 0x821;
const ROCK_A: u32 = 0x830;
const BARREL_A: u32 = 0x831;
const MARKER_A: u32 = 0x832;
const ROCK_B: u32 = 0x840;
const PLAIN_DOOR: u32 = 0x850;
const PLAYER: u32 = 0x14;

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: [x, y, z],
        rotation: [0.0; 3],
    }
}

fn placed(id: u32, base: u32, t: Transform) -> Record {
    Record::new(T(b"REFR"), id)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), t.to_bytes())
}

/// A door that teleports: `XTEL` names the far door and where to land.
fn load_door(id: u32, base: u32, here: Transform, far: u32, landing: Transform) -> Record {
    let mut xtel = far.to_le_bytes().to_vec();
    xtel.extend_from_slice(&landing.to_bytes());
    xtel.extend_from_slice(&0u32.to_le_bytes());
    placed(id, base, here).sub(T(b"XTEL"), xtel)
}

/// An interior cell with a temporary child group and a persistent one, which
/// is the split vanilla draws and the split an unload respects.
fn cell(
    p: &mut Plugin,
    id: u32,
    edid: &str,
    temporary: Vec<Record>,
    persistent: Vec<Record>,
) -> Group {
    let rec = p.push_record(
        Record::new(T(b"CELL"), id)
            .zsub(EDID, edid)
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut children = Group::new(GroupKind::CellChildren(id));
    if !persistent.is_empty() {
        let mut g = Group::new(GroupKind::CellPersistent(id));
        for r in persistent {
            let idx = p.push_record(r);
            g.entries.push(Entry::Record(idx));
        }
        children.entries.push(Entry::Group(g));
    }
    let mut temp = Group::new(GroupKind::CellTemporary(id));
    for r in temporary {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(rec));
    sub.entries.push(Entry::Group(children));
    sub
}

fn test_plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(Record::new(T(b"DOOR"), DOOR_BASE).zsub(EDID, "TestDoor"));
    p.push_top(Record::new(T(b"STAT"), STATIC_BASE).zsub(EDID, "TestStatic"));
    p.push_top(Record::new(T(b"NPC_"), PLAYER_BASE).zsub(EDID, "PlayerBase"));

    // Cell A: the player, two statics, a load door out, a plain door, and a
    // persistent marker. Cell B: one static and the load door back.
    let a = cell(
        &mut p,
        CELL_A,
        "TestCellA",
        vec![
            load_door(
                DOOR_A,
                DOOR_BASE,
                at(0.0, 0.0, 0.0),
                DOOR_B,
                at(500.0, 10.0, 20.0),
            ),
            placed(ROCK_A, STATIC_BASE, at(10.0, 0.0, 0.0)),
            placed(BARREL_A, STATIC_BASE, at(20.0, 0.0, 0.0)),
            placed(PLAIN_DOOR, DOOR_BASE, at(30.0, 0.0, 0.0)),
            placed(PLAYER, PLAYER_BASE, at(1.0, 1.0, 1.0)),
        ],
        vec![placed(MARKER_A, STATIC_BASE, at(40.0, 0.0, 0.0))],
    );
    let b = cell(
        &mut p,
        CELL_B,
        "TestCellB",
        vec![
            load_door(
                DOOR_B,
                DOOR_BASE,
                at(500.0, 0.0, 0.0),
                DOOR_A,
                at(0.0, 5.0, 5.0),
            ),
            placed(ROCK_B, STATIC_BASE, at(510.0, 0.0, 0.0)),
        ],
        vec![],
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(a));
    block.entries.push(Entry::Group(b));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

pub fn game() -> Game {
    let bytes = test_plugin().to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Test.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    Game::new(order)
}

pub fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

/// The ids a sibling test needs to drive a transition without rebuilding the
/// fixture: the cell to start in, the door to walk through, and the door back.
#[allow(dead_code)]
pub const START_CELL: u32 = CELL_A;
#[allow(dead_code)]
pub const DOOR_OUT: u32 = DOOR_A;
// `no_load_screen.rs` pulls this file in with `#[path]`, so it compiles a
// second time as a module there and only takes the two ids it needs. The
// return door is used by this file's own tests, which is invisible from
// inside that second build.
#[allow(dead_code)]
pub const DOOR_BACK: u32 = DOOR_B;

fn resident(g: &Game, object: u32) -> bool {
    g.world().reference_ref(id(object)).is_some()
}

#[test]
fn a_load_door_moves_the_player_and_swaps_the_cell() {
    let mut g = game();
    let player = g.world_mut().player();
    assert_eq!(player, id(PLAYER), "the shipped player reference is used");

    // Start resident in cell A, the way arriving there would leave it.
    let built = g.world_mut().load_cell(id(CELL_A));
    assert!(built > 0, "cell A built {built} references");
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_A)));
    assert!(resident(&g, ROCK_A) && resident(&g, BARREL_A));

    assert!(g.activate(id(DOOR_A), player));

    // The player landed where the door's teleport data says, in cell B.
    let w = g.world();
    let p = w.reference_ref(id(PLAYER)).unwrap();
    assert_eq!(p.position, [500.0, 10.0, 20.0], "landed at the far side");
    assert_eq!(p.cell, Some(id(CELL_B)), "and in the far cell");
    drop(w);

    assert_eq!(g.world().loaded_cell(), Some(id(CELL_B)));
    assert!(resident(&g, ROCK_B), "the destination's references loaded");
    assert!(!resident(&g, ROCK_A), "the old cell's rock unloaded");
    assert!(!resident(&g, BARREL_A), "and its barrel");
    assert!(resident(&g, PLAYER), "the player is never unloaded");
}

#[test]
fn a_persistent_reference_survives_the_unload() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));
    assert!(resident(&g, MARKER_A), "the persistent marker loaded");

    g.activate(id(DOOR_A), player);

    assert!(
        resident(&g, MARKER_A),
        "a persistent child is not the cell's to unload"
    );
}

#[test]
fn a_reference_a_script_changed_is_kept_rather_than_rebuilt() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));

    // Two changes a script could have made, one to each rock.
    g.world_mut().reference(id(ROCK_A)).unwrap().enabled = false;
    g.world_mut().reference(id(BARREL_A)).unwrap().position = [999.0, 0.0, 0.0];

    g.activate(id(DOOR_A), player);

    assert!(
        resident(&g, ROCK_A),
        "a disabled reference would come back enabled, so it stays"
    );
    assert!(
        resident(&g, BARREL_A),
        "a moved reference would come back in place, so it stays"
    );
    assert!(!g.world().reference_ref(id(ROCK_A)).unwrap().enabled);
    assert_eq!(
        g.world().reference_ref(id(BARREL_A)).unwrap().position,
        [999.0, 0.0, 0.0]
    );
}

#[test]
fn a_door_without_teleport_data_opens_instead_of_moving_anyone() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));
    let before = g.world_mut().reference(id(PLAYER)).unwrap().cell;

    assert!(g.activate(id(PLAIN_DOOR), player));

    assert!(
        g.world().reference_ref(id(PLAIN_DOOR)).unwrap().opened,
        "a plain door swings"
    );
    assert_eq!(
        g.world_mut().reference(id(PLAYER)).unwrap().cell,
        before,
        "and nobody moved"
    );
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_A)), "no transition");
}

#[test]
fn the_clock_keeps_running_across_a_transition() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));
    for _ in 0..30 {
        g.tick(1.0 / 60.0);
    }
    let before = g.world().clock.real_seconds;
    let days_before = g.world().clock.game_days;

    g.activate(id(DOOR_A), player);
    for _ in 0..30 {
        g.tick(1.0 / 60.0);
    }

    let after = g.world().clock.real_seconds;
    assert!(
        after > before,
        "real time advanced across the door, {before} to {after}"
    );
    assert!(
        g.world().clock.game_days >= days_before,
        "and game time did not go backwards"
    );
}

#[test]
fn going_back_through_the_far_door_returns_and_reloads() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));

    g.activate(id(DOOR_A), player);
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_B)));

    g.activate(id(DOOR_B), player);

    assert_eq!(g.world().loaded_cell(), Some(id(CELL_A)));
    assert_eq!(
        g.world().reference_ref(id(PLAYER)).unwrap().position,
        [0.0, 5.0, 5.0],
        "landed back at cell A's door"
    );
    assert!(resident(&g, ROCK_A), "cell A's references came back");
    assert!(!resident(&g, ROCK_B), "and cell B's went away");
}

#[test]
fn changing_to_the_cell_already_resident_does_nothing() {
    let g = game();
    g.world_mut().load_cell(id(CELL_A));
    let moved = g.world_mut().change_cell(Some(id(CELL_A)));
    assert_eq!(moved.unloaded, 0);
    assert_eq!(moved.loaded, 0);
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_A)));
}

#[test]
fn a_transition_survives_a_save_and_load() {
    let mut g = game();
    let player = g.world_mut().player();
    g.world_mut().load_cell(id(CELL_A));
    g.activate(id(DOOR_A), player);

    let saved = g.world().save_state();
    let fresh = game();
    fresh.world_mut().load_state(&saved).unwrap();

    assert_eq!(
        fresh.world().loaded_cell(),
        Some(id(CELL_B)),
        "a loaded game knows which cell it is holding"
    );
}
