// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-28: the actors a host contact query can strike follow the shared
//! World across a load door, and melee reach is the UESP WEAP formula
//! `fCombatDistance * NPCScale * WeaponReach`.
//!
//! Two interior cells, each with an `ACHR` actor. The player is an `ACHR`
//! too, so the attacker appears in the list and the host excludes it by owner.

use soe_esm::records::Transform;
use soe_esm::{plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder};
use soe_esm::{Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::world::DEFAULT_COMBAT_DISTANCE;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const DOOR_BASE: u32 = 0x800;
const PLAYER_BASE: u32 = 0x802;
const GUARD_BASE: u32 = 0x803;
const CELL_A: u32 = 0x810;
const CELL_B: u32 = 0x811;
const DOOR_A: u32 = 0x820;
const DOOR_B: u32 = 0x821;
const GUARD_A: u32 = 0x860;
const GUARD_B: u32 = 0x861;
const SWORD: u32 = 0xa00;
const PLAYER: u32 = 0x14;

const SWORD_REACH: f32 = 1.25;
const PLAYER_SCALE: f32 = 1.1;
const GUARD_A_SCALE: f32 = 1.5;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn at(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        position: [x, y, z],
        rotation: [0.0; 3],
    }
}

fn door(id: u32, here: Transform, far: u32, landing: Transform) -> Record {
    let mut xtel = far.to_le_bytes().to_vec();
    xtel.extend_from_slice(&landing.to_bytes());
    xtel.extend_from_slice(&0u32.to_le_bytes());
    Record::new(T(b"REFR"), id)
        .sub(T(b"NAME"), DOOR_BASE.to_le_bytes())
        .sub(T(b"DATA"), here.to_bytes())
        .sub(T(b"XTEL"), xtel)
}

fn actor(id: u32, base: u32, t: Transform, scale: Option<f32>) -> Record {
    let record = Record::new(T(b"ACHR"), id)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), t.to_bytes());
    match scale {
        Some(scale) => record.sub(T(b"XSCL"), scale.to_le_bytes()),
        None => record,
    }
}

fn cell(p: &mut Plugin, id: u32, edid: &str, temporary: Vec<Record>) -> Group {
    let rec = p.push_record(
        Record::new(T(b"CELL"), id)
            .zsub(EDID, edid)
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut children = Group::new(GroupKind::CellChildren(id));
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

/// A one-handed sword whose DNAM reach is `reach`, in the right-hand slot.
fn sword(reach: f32) -> Record {
    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&reach.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    Record::new(T(b"WEAP"), SWORD)
        .sub(T(b"DATA"), data)
        .sub(T(b"DNAM"), dnam)
        .sub(T(b"ETYP"), 0xb02_u32.to_le_bytes())
}

/// Hand slots and the default hand objects `equip_hand_weapon` resolves.
fn equipment(p: &mut Plugin) {
    for slot in [0xb00_u32, 0xb01, 0xb02] {
        let mut record = Record::new(T(b"EQUP"), slot).sub(T(b"DATA"), 0_u32.to_le_bytes());
        if slot == 0xb02 {
            let mut parents = 0xb00_u32.to_le_bytes().to_vec();
            parents.extend_from_slice(&0xb01_u32.to_le_bytes());
            record = record.sub(T(b"PNAM"), parents);
        }
        p.push_top(record);
    }
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    p.push_top(Record::new(T(b"DOBJ"), 0x31).sub(T(b"DNAM"), defaults));
}

fn plugin(combat_distance: Option<f32>) -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(Record::new(T(b"DOOR"), DOOR_BASE).zsub(EDID, "TestDoor"));
    p.push_top(Record::new(T(b"NPC_"), PLAYER_BASE).zsub(EDID, "PlayerBase"));
    p.push_top(Record::new(T(b"NPC_"), GUARD_BASE).zsub(EDID, "Guard"));
    equipment(&mut p);
    p.push_top(sword(SWORD_REACH));
    if let Some(value) = combat_distance {
        p.push_top(
            Record::new(T(b"GMST"), 0x1000)
                .zsub(EDID, "fCombatDistance")
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    let a = cell(
        &mut p,
        CELL_A,
        "ContactCellA",
        vec![
            door(DOOR_A, at(0.0, 0.0, 0.0), DOOR_B, at(500.0, 10.0, 20.0)),
            actor(PLAYER, PLAYER_BASE, at(1.0, 1.0, 1.0), Some(PLAYER_SCALE)),
            actor(
                GUARD_A,
                GUARD_BASE,
                at(100.0, 0.0, 0.0),
                Some(GUARD_A_SCALE),
            ),
        ],
    );
    let b = cell(
        &mut p,
        CELL_B,
        "ContactCellB",
        vec![
            door(DOOR_B, at(500.0, 0.0, 0.0), DOOR_A, at(0.0, 5.0, 5.0)),
            actor(GUARD_B, GUARD_BASE, at(520.0, 0.0, 0.0), None),
        ],
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(a));
    block.entries.push(Entry::Group(b));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

fn game(combat_distance: Option<f32>) -> Game {
    let bytes = plugin(combat_distance).to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Contact.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let game = Game::new(order);
    let built = game.world_mut().load_cell(id(CELL_A));
    assert!(built > 0, "cell A built {built} references");
    game
}

fn listed(game: &Game) -> Vec<u32> {
    game.world_mut()
        .contact_actors()
        .iter()
        .map(|a| a.actor.object)
        .collect()
}

#[test]
fn contact_actors_follow_the_resident_cell_through_a_load_door() {
    let mut g = game(None);
    let player = g.world_mut().player();
    assert_eq!(player, id(PLAYER));

    let actors = g.world_mut().contact_actors();
    let objects: Vec<u32> = actors.iter().map(|a| a.actor.object).collect();
    assert_eq!(objects, vec![PLAYER, GUARD_A], "cell A actors only, sorted");
    let guard = actors.iter().find(|a| a.actor == id(GUARD_A)).unwrap();
    assert_eq!(guard.cell, id(CELL_A));
    assert_eq!(guard.position, [100.0, 0.0, 0.0]);
    assert!(
        (guard.scale - GUARD_A_SCALE).abs() < 1e-6,
        "no race height, so the capsule scale is the XSCL alone: {}",
        guard.scale
    );

    assert!(g.activate(id(DOOR_A), player));
    assert_eq!(g.world().loaded_cell(), Some(id(CELL_B)));
    let after = listed(&g);
    assert!(after.contains(&GUARD_B), "the far cell's actor is listed");
    assert!(!after.contains(&GUARD_A), "the left cell's actor is gone");
    assert!(after.contains(&PLAYER), "the player travels with the cell");
    let player_entry = g
        .world_mut()
        .contact_actors()
        .into_iter()
        .find(|a| a.actor == id(PLAYER))
        .unwrap();
    assert_eq!(player_entry.cell, id(CELL_B));
    assert_eq!(player_entry.position, [500.0, 10.0, 20.0]);
}

#[test]
fn a_disabled_or_deleted_actor_leaves_the_contact_list() {
    let g = game(None);
    g.world_mut().reference(id(GUARD_A)).unwrap().enabled = false;
    assert_eq!(listed(&g), vec![PLAYER], "disabled guard dropped");
    {
        let mut world = g.world_mut();
        let guard = world.reference(id(GUARD_A)).unwrap();
        guard.enabled = true;
        guard.deleted = true;
    }
    assert_eq!(listed(&g), vec![PLAYER], "deleted guard dropped");
}

#[test]
fn a_moved_actor_reports_its_new_placement() {
    let g = game(None);
    g.world_mut().reference(id(GUARD_A)).unwrap().position = [120.0, 30.0, 0.0];
    let guard = g
        .world_mut()
        .contact_actors()
        .into_iter()
        .find(|a| a.actor == id(GUARD_A))
        .unwrap();
    assert_eq!(guard.position, [120.0, 30.0, 0.0]);
}

#[test]
fn a_non_finite_placement_is_skipped_rather_than_sent_to_the_host() {
    let g = game(None);
    g.world_mut().reference(id(GUARD_A)).unwrap().position = [f32::NAN, 0.0, 0.0];
    assert_eq!(listed(&g), vec![PLAYER]);
}

#[test]
fn melee_reach_is_combat_distance_times_scale_times_weapon_reach() {
    let g = game(Some(150.0));
    let player = g.world_mut().player();
    {
        let mut world = g.world_mut();
        world.add_item(player, id(SWORD), 1);
        world
            .equip_hand_weapon(player, Hand::Right, id(SWORD))
            .unwrap();
    }
    let reach = g.world_mut().melee_reach(player, Hand::Right).unwrap();
    let expected = 150.0 * PLAYER_SCALE * SWORD_REACH;
    assert!(
        (reach - expected).abs() < 1e-3,
        "reach {reach}, expected {expected}"
    );
    assert!(
        g.world_mut().melee_reach(player, Hand::Left).is_err(),
        "an empty hand has no reach"
    );
}

#[test]
fn melee_reach_falls_back_to_the_vanilla_combat_distance() {
    let g = game(None);
    let player = g.world_mut().player();
    {
        let mut world = g.world_mut();
        world.add_item(player, id(SWORD), 1);
        world
            .equip_hand_weapon(player, Hand::Right, id(SWORD))
            .unwrap();
    }
    let reach = g.world_mut().melee_reach(player, Hand::Right).unwrap();
    let expected = DEFAULT_COMBAT_DISTANCE * PLAYER_SCALE * SWORD_REACH;
    assert!((reach - expected).abs() < 1e-3, "reach {reach}");
}
