// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MGEF Open unlocks references within its magnitude threshold.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::magic::MagicError;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOURCE: u32 = 0x100;
const ACTOR_BASE: u32 = 0x101;
const DOOR_BASE: u32 = 0x102;
const CONTAINER_BASE: u32 = 0x104;
const OWNER: u32 = 0x103;
const DOOR: u32 = 0x200;
const CONTAINER: u32 = 0x201;
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
    plugin.push_top(Record::new(T(b"FACT"), OWNER));
    let mut lock = vec![0; 20];
    lock[0] = 4;
    plugin.push_top(
        Record::new(T(b"REFR"), DOOR)
            .sub(T(b"NAME"), DOOR_BASE.to_le_bytes())
            .sub(T(b"XOWN"), OWNER.to_le_bytes())
            .sub(T(b"XLOC"), lock),
    );
    let mut container_lock = vec![0; 20];
    container_lock[0] = 4;
    plugin.push_top(
        Record::new(T(b"REFR"), CONTAINER)
            .sub(T(b"NAME"), CONTAINER_BASE.to_le_bytes())
            .sub(T(b"XOWN"), OWNER.to_le_bytes())
            .sub(T(b"XLOC"), container_lock),
    );
    let mut mgef = vec![0; 152];
    put(&mut mgef, 64, 16);
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

fn cast(game: &mut Game, target: u32) -> Result<Vec<soe_game::world::magic::EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(target)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

#[test]
fn mag_37_open_unlocks_locks_at_or_below_its_magnitude() {
    let mut game = fixture(4.0);

    assert!(cast(&mut game, DOOR).unwrap().is_empty());
    assert!(cast(&mut game, CONTAINER).unwrap().is_empty());
    let mut world = game.world_mut();
    let door = world.reference(id(DOOR)).unwrap();
    assert!(!door.locked);
    assert_eq!(door.lock_level, 4);
    assert_eq!(door.owner, Some(id(OWNER)));
    let container = world.reference(id(CONTAINER)).unwrap();
    assert!(!container.locked);
    assert_eq!(container.owner, Some(id(OWNER)));
    assert_eq!(
        world
            .pending
            .iter()
            .filter(|pending| matches!(pending, soe_game::world::Pending::Event { name, .. } if name == "OnLockStateChanged"))
            .count(),
        2
    );
    assert_eq!(world.active_effects().count(), 0);
}

#[test]
fn mag_37_open_preserves_locks_above_its_magnitude() {
    let mut game = fixture(3.99);

    assert!(cast(&mut game, DOOR).unwrap().is_empty());
    let mut world = game.world_mut();
    let door = world.reference(id(DOOR)).unwrap();
    assert!(door.locked);
    assert_eq!(door.lock_level, 4);
    assert_eq!(door.owner, Some(id(OWNER)));
}

#[test]
fn mag_37_open_rejects_actor_targets_without_mutating_locks() {
    let mut game = fixture(100.0);
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(SOURCE)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(matches!(result, Err(MagicError::Target)));
    let mut world = game.world_mut();
    let door = world.reference(id(DOOR)).unwrap();
    assert!(door.locked);
    assert_eq!(door.lock_level, 4);
    assert_eq!(door.owner, Some(id(OWNER)));
    assert!(world.pending.is_empty());
}
