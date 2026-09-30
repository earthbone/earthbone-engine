// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(n: u32) -> GlobalFormId { GlobalFormId { file: 0, object: n } }
fn put(d: &mut [u8], n: usize, v: u32) { d[n..n + 4].copy_from_slice(&v.to_le_bytes()); }

#[test]
fn mag_54_stagger_uses_cast_source_when_caster_is_absent() {
    let mut effect = vec![0; 152]; put(&mut effect, 64, 33); put(&mut effect, 68, 24); put(&mut effect, 80, 1); put(&mut effect, 84, 3);
    let mut spell = vec![0; 36]; put(&mut spell, 16, 1); put(&mut spell, 20, 3);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [0x100, 0x101] { plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x500u32.to_le_bytes())); }
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), effect));
    plugin.push_top(Record::new(T(b"SPEL"), 0x300).sub(T(b"SPIT"), spell)
        .sub(T(b"EFID"), 0x200u32.to_le_bytes())
        .sub(T(b"EFIT"), [0.4f32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat()));
    plugin.push_top(Record::new(T(b"GMST"), 0x400).zsub(T(b"EDID"), "fStaggerDuration").sub(T(b"DATA"), 1.2f32.to_le_bytes()));
    let mut order = LoadOrder::new(); order.add("Stagger.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.shared_world().borrow_mut().cast_spell(id(0x300), id(0x100), None, Some(id(0x101)),
        &mut |name, class| game.vm.create_object_of(name, class)).unwrap();
    let world = game.world();
    let actor = world.actor_ref(id(0x101)).unwrap();
    assert_eq!(actor.stagger_magnitude, f64::from(0.4_f32));
    assert_eq!(actor.stagger_direction, Some(id(0x100)));
    assert_eq!(actor.stagger_end_time, Some(f64::from(1.2_f32)));
}
