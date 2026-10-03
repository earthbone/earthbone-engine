// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::MagicWorldState;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{commonlib_magic::CommonLibMagic, World};

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

#[test]
fn shm_13_favorite_state_survives_save_round_trip_and_removal() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x100));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x101).sub(Tag(*b"SPIT"), vec![0; 36]));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x200).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let mut world = World::new(order);
    let actor = FormId(world.order().runtime_id(id(0x200)));
    let spell = FormId(world.order().runtime_id(id(0x101)));
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    CommonLibMagic::new(&mut world, &mut alloc).set_favorite(actor, spell, true).unwrap();
    let bytes = world.save_state();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x100));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x101).sub(Tag(*b"SPIT"), vec![0; 36]));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x200).sub(Tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut load_order = LoadOrder::new();
    load_order.add("Magic.esm", plugin).unwrap();
    let mut restored = World::new(load_order);
    restored.load_state(&bytes).unwrap();
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    let mut adapter = CommonLibMagic::new(&mut restored, &mut alloc);
    assert!(adapter.is_favorite(actor, spell).unwrap());
    assert!(adapter.set_favorite(actor, spell, false).unwrap());
    assert!(!adapter.is_favorite(actor, spell).unwrap());
}
