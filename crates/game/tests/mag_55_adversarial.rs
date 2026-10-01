// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn peak(effect: u32, amount: f32, duration: u32) -> (Record, Record) {
    let mut data = vec![0; 152];
    if amount < 0.0 {
        put(&mut data, 0, 1 << 2);
    }
    put(&mut data, 8, 0x400);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 34);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    let mgef = Record::new(T(b"MGEF"), effect).sub(T(b"DATA"), data);
    let spell = Record::new(T(b"SPEL"), effect + 0x100)
        .sub(T(b"SPIT"), {
            let mut d = vec![0; 36];
            put(&mut d, 16, 1);
            put(&mut d, 20, 3);
            d
        })
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(
            T(b"EFIT"),
            [
                amount.abs().to_le_bytes(),
                0u32.to_le_bytes(),
                duration.to_le_bytes(),
            ]
            .concat(),
        );
    (mgef, spell)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"ACHR"), 0x100).sub(T(b"NAME"), 0x500u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"KYWD"), 0x400));
    let (a, sa) = peak(0x200, -25.0, 10);
    let (b, sb) = peak(0x201, -10.0, 5);
    plugin.push_top(a);
    plugin.push_top(sa);
    plugin.push_top(b);
    plugin.push_top(sb);
    let mut order = LoadOrder::new();
    order.add("Peak.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x100), "health", 100.0)
        .unwrap();
    game
}

fn cast(game: &mut Game, spell: u32) {
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(0x100),
            Some(id(0x100)),
            Some(id(0x100)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_55_peak_group_compares_signed_negative_modifiers_and_promotes_after_expiry() {
    let mut game = fixture();
    cast(&mut game, 0x300);
    cast(&mut game, 0x301);
    assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 90.0);
    game.tick(5.0);
    assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 75.0);
    game.tick(5.0);
    assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
