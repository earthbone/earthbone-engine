// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{MagicWorldState, UnsupportedState};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{commonlib_magic::CommonLibMagic, World};

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 1));
    plugin.push_top(Record::new(Tag(*b"MISC"), 2));
    for actor in [0x20, 0x21] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 1_u32.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("shm_13_extra.esp", plugin).unwrap();
    World::new(order)
}

fn allocate(_: &str, _: &str) -> soe_papyrus::ObjectId {
    soe_papyrus::ObjectId(1)
}

#[test]
fn favorite_sets_are_per_actor_and_unknown_runtime_ids_are_rejected() {
    let mut world = fixture();
    let a = id(0, 0x20);
    let b = id(0, 0x21);
    world.actor(a).unwrap();
    world.actor(b).unwrap();
    let a_form = FormId(world.order().runtime_id(a));
    let b_form = FormId(world.order().runtime_id(b));
    let item = FormId(world.order().runtime_id(id(0, 2)));
    let mut alloc = allocate;
    let mut api = CommonLibMagic::new(&mut world, &mut alloc);

    assert!(api.set_favorite(a_form, item, true).unwrap());
    assert!(api.is_favorite(a_form, item).unwrap());
    assert!(!api.is_favorite(b_form, item).unwrap());
    assert!(matches!(api.is_favorite(FormId(0xDEAD_BEEF), item), Err(UnsupportedState::MissingReference)));
    assert!(matches!(api.set_favorite(a_form, FormId(0xDEAD_BEEF), true), Err(UnsupportedState::MissingReference)));
}

#[test]
fn unsupported_marker_bits_do_not_partially_change_live_marker_state() {
    let mut world = fixture();
    let reference = id(0, 0x20);
    world.reference(reference).unwrap();
    let form = FormId(world.order().runtime_id(reference));
    let mut alloc = allocate;
    let mut api = CommonLibMagic::new(&mut world, &mut alloc);

    api.set_map_marker_flags(form, 1).unwrap();
    assert!(api.set_map_marker_flags(form, 0b100).is_err());
    assert_eq!(api.map_marker_flags(form).unwrap(), 1);
    assert!(matches!(
        api.set_map_marker_flags(FormId(0xDEAD_BEEF), 0),
        Err(UnsupportedState::MissingReference)
    ));
}
