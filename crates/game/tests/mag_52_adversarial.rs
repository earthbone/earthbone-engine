// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(n: u32) -> GlobalFormId { GlobalFormId { file: 0, object: n } }
fn put(d: &mut [u8], n: usize, v: u32) { d[n..n + 4].copy_from_slice(&v.to_le_bytes()); }

#[test]
fn mag_52_value_and_parts_ignores_unused_association_and_only_modifies_primary_value() {
    let mut data = vec![0; 152];
    put(&mut data, 0, 1 << 1); // Recoverable modifier.
    put(&mut data, 8, 0x700); // UESP and xEdit identify this field as unused.
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 31);
    put(&mut data, 68, 24); // Health.
    put(&mut data, 72, 25); // Must not change secondary AV.
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    let mut spell_data = vec![0; 36]; put(&mut spell_data, 16, 1); put(&mut spell_data, 20, 3);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [0x100, 0x101] { plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x500u32.to_le_bytes())); }
    plugin.push_top(Record::new(T(b"MISC"), 0x700));
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), data));
    plugin.push_top(Record::new(T(b"SPEL"), 0x300).sub(T(b"SPIT"), spell_data)
        .sub(T(b"EFID"), 0x200u32.to_le_bytes())
        .sub(T(b"EFIT"), [12.0f32.to_le_bytes(), 0u32.to_le_bytes(), 5u32.to_le_bytes()].concat()));
    let mut order = LoadOrder::new(); order.add("ValueParts.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [0x100, 0x101] {
        game.world_mut().set_actor_value(id(actor), "health", 100.0).unwrap();
        game.world_mut().set_actor_value(id(actor), "magicka", 100.0).unwrap();
    }
    game.shared_world().borrow_mut().cast_spell(id(0x300), id(0x100), Some(id(0x100)), Some(id(0x101)),
        &mut |name, class| game.vm.create_object_of(name, class)).unwrap();
    assert_eq!(game.world_mut().actor_value(id(0x101), "health"), 112.0);
    assert_eq!(game.world_mut().actor_value(id(0x101), "magicka"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 100.0);
    game.tick(5.0);
    assert_eq!(game.world_mut().actor_value(id(0x101), "health"), 100.0);
}
