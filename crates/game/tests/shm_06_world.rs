// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world::{
    BGSSaveLoadGame, BGSSaveLoadManager, NamedUnsupportedHook, WorldBindings,
};
use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn test_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let entry = Record::new(T(b"WEAP"), 0x200);
    let mut bytes = vec![0; 12];
    bytes[..2].copy_from_slice(&1_u16.to_le_bytes());
    bytes[4..8].copy_from_slice(&0x200_u32.to_le_bytes());
    bytes[8..10].copy_from_slice(&2_u16.to_le_bytes());
    let mut extra = Vec::new();
    extra.extend_from_slice(&0x300_u32.to_le_bytes());
    extra.extend_from_slice(&u32::MAX.to_le_bytes());
    extra.extend_from_slice(&0.5_f32.to_bits().to_le_bytes());
    let list = Record::new(T(b"LVLI"), 0x100)
        .sub(T(b"LVLD"), [0])
        .sub(T(b"LVLF"), [4])
        .sub(T(b"LLCT"), [1])
        .sub(T(b"LVLO"), bytes)
        .sub(T(b"COED"), extra);
    plugin.push_top(entry);
    plugin.push_top(Record::new(T(b"FACT"), 0x300));
    plugin.push_top(list);
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn shm_06_save_load_manager_calls_the_world_save_state() {
    let mut world = test_world();
    let saved = world.save();
    assert!(!saved.is_empty());
    world.load(&saved).unwrap();
    assert_eq!(world.save_state(), saved);
}

#[test]
fn shm_06_calced_object_comes_from_the_live_leveled_list() {
    let mut world = test_world();
    let objects = world
        .calculate_current_form_list(0x0000_0100, 1, 1)
        .unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].form_id, 0x0000_0200);
    assert_eq!(objects[0].count, 2);
    assert_eq!(objects[0].container_item.owner, Some(0x0000_0300));
    assert_eq!(objects[0].container_item.owner_rank, None);
    assert_eq!(
        objects[0].container_item.health_bits,
        Some(0.5_f32.to_bits())
    );
}

#[test]
fn shm_06_unsupported_world_hooks_are_named_and_do_not_mutate_state() {
    let mut world = test_world();
    let before = world.save_state();
    assert_eq!(
        world.beam_projectile_impact(),
        Err(NamedUnsupportedHook("BeamProjectile::AddImpact"))
    );
    assert_eq!(
        world.character_flags(),
        Err(NamedUnsupportedHook("bhkCharacterController::flags"))
    );
    assert_eq!(
        world.get_change(),
        Err(NamedUnsupportedHook("BGSSaveLoadGame::GetChange"))
    );
    assert_eq!(
        world.is_form_id_in_use(0x1234),
        Err(NamedUnsupportedHook("BGSSaveLoadGame::IsFormIDInUse"))
    );
    assert_eq!(
        world.save_file(),
        Err(NamedUnsupportedHook("BGSSaveLoadManager::Save"))
    );
    assert_eq!(
        world.load_most_recent_save_game(),
        Err(NamedUnsupportedHook(
            "BGSSaveLoadManager::LoadMostRecentSaveGame"
        ))
    );
    assert_eq!(world.save_state(), before);
}

#[test]
fn shm_06_manager_load_rejects_corrupt_state_without_losing_live_state() {
    let mut world = test_world();
    let before = world.save_state();
    assert!(world.load(&[0, 1, 2]).is_err());
    assert_eq!(world.save_state(), before);
}
