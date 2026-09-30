// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(n: u32) -> GlobalFormId { GlobalFormId { file: 0, object: n } }
fn put(d: &mut [u8], n: usize, v: u32) { d[n..n + 4].copy_from_slice(&v.to_le_bytes()); }

#[test]
fn mag_58_slow_time_keeps_real_clock_running_and_pauses_both_domains_in_menu() {
    let mut effect = vec![0; 152]; put(&mut effect, 64, 37); put(&mut effect, 68, 24); put(&mut effect, 80, 1); put(&mut effect, 84, 3);
    let mut spell = vec![0; 36]; put(&mut spell, 16, 1); put(&mut spell, 20, 3);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"ACHR"), 0x100).sub(T(b"NAME"), 0x500u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), effect));
    plugin.push_top(Record::new(T(b"SPEL"), 0x300).sub(T(b"SPIT"), spell)
        .sub(T(b"EFID"), 0x200u32.to_le_bytes())
        .sub(T(b"EFIT"), [90.0f32.to_le_bytes(), 0u32.to_le_bytes(), 10u32.to_le_bytes()].concat()));
    let mut order = LoadOrder::new(); order.add("SlowTime.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.shared_world().borrow_mut().cast_spell(id(0x300), id(0x100), Some(id(0x100)), Some(id(0x100)),
        &mut |name, class| game.vm.create_object_of(name, class)).unwrap();
    game.tick(2.0);
    assert_eq!(game.world().clock.real_seconds, 2.0);
    assert!((game.world().clock.game_days - 4.0 / 86_400.0).abs() < 1e-12);
    let days = game.world().clock.game_days;
    game.tick_menu_mode(1.0, true);
    assert_eq!(game.world().clock.real_seconds, 2.0);
    assert_eq!(game.world().clock.game_days, days);
}
