// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MGEF Lock execution through the shared reference lock state.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::MagicError;
use soe_game::world::{Pending, World};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOURCE: u32 = 0x100;
const ACTOR_BASE: u32 = 0x101;
const DOOR_BASE: u32 = 0x102;
const CONTAINER_BASE: u32 = 0x103;
const ACTIVATOR_BASE: u32 = 0x104;
const DOOR_REF: u32 = 0x200;
const CHEST_REF: u32 = 0x201;
const ACTIVATOR_REF: u32 = 0x202;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(magnitude: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), SOURCE)
            .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"DOOR"), DOOR_BASE));
    plugin.push_top(Record::new(T(b"CONT"), CONTAINER_BASE));
    plugin.push_top(Record::new(T(b"ACTI"), ACTIVATOR_BASE));
    let mut xloc = vec![0; 20];
    xloc[0] = 2;
    for (reference, base, lock) in [
        (DOOR_REF, DOOR_BASE, Some(xloc.clone())),
        (CHEST_REF, CONTAINER_BASE, Some(xloc)),
        (ACTIVATOR_REF, ACTIVATOR_BASE, None),
    ] {
        let mut record = Record::new(T(b"REFR"), reference).sub(T(b"NAME"), base.to_le_bytes());
        if let Some(lock) = lock {
            record = record.sub(T(b"XLOC"), lock);
        }
        plugin.push_top(record);
    }
    let mut mgef = vec![0; 152];
    put(&mut mgef, 64, 15);
    put(&mut mgef, 80, 1);
    put(&mut mgef, 84, 3);
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));
    let mut spit = vec![0; 36];
    put(&mut spit, 8, 0);
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(game: &mut Game, target: u32) -> Result<(), MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(target)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result.map(|_| ())
}

fn lock_changed(world: &World) -> usize {
    world
        .pending
        .iter()
        .filter(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnLockStateChanged"))
        .count()
}

#[test]
fn mag_36_lock_updates_a_door_level_through_shared_lock_state() {
    let mut game = fixture(6.6);
    cast(&mut game, DOOR_REF).unwrap();

    let mut world = game.world_mut();
    let lock = world.reference(id(DOOR_REF)).unwrap();
    assert_eq!(lock.lock_level, 7);
    assert!(lock.locked);
    assert_eq!(lock_changed(&world), 1);
    assert_eq!(world.active_effects().count(), 0);
}

#[test]
fn mag_36_lock_updates_a_container_reference() {
    let mut game = fixture(4.0);
    cast(&mut game, CHEST_REF).unwrap();

    let mut world = game.world_mut();
    let lock = world.reference(id(CHEST_REF)).unwrap();
    assert_eq!(lock.lock_level, 4);
    assert!(lock.locked);
    assert_eq!(lock_changed(&world), 1);
    assert_eq!(world.active_effects().count(), 0);
}

#[test]
fn mag_36_lock_rejects_actor_and_non_lockable_reference_targets() {
    let mut game = fixture(8.0);
    assert!(matches!(cast(&mut game, SOURCE), Err(MagicError::Target)));
    assert!(matches!(
        cast(&mut game, ACTIVATOR_REF),
        Err(MagicError::Target)
    ));
    assert_eq!(lock_changed(&game.world()), 0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_36_lock_repeated_same_level_does_not_emit_a_second_change_event() {
    let mut game = fixture(6.0);
    cast(&mut game, DOOR_REF).unwrap();
    cast(&mut game, DOOR_REF).unwrap();
    assert_eq!(lock_changed(&game.world()), 1);
}

#[test]
fn mag_36_lock_level_and_locked_state_survive_world_save_load() {
    let mut original = fixture(12.0);
    cast(&mut original, DOOR_REF).unwrap();
    let saved = original.world().save_state();
    let restored = fixture(12.0);
    restored.world_mut().load_state(&saved).unwrap();
    let mut world = restored.world_mut();
    let door = world.reference(id(DOOR_REF)).unwrap();
    assert_eq!(door.lock_level, 12);
    assert!(door.locked);
}
