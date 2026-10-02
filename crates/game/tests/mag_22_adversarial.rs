// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(cast_type: u32) -> Game {
    let mut mgef_data = vec![0; 152];
    put(&mut mgef_data, 0, 1 << 1);
    put(&mut mgef_data, 16, u32::MAX);
    put(&mut mgef_data, 64, 0);
    put(&mut mgef_data, 68, 24);
    put(&mut mgef_data, 80, cast_type);
    put(&mut mgef_data, 84, 0);
    let mgef = Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef_data);
    let mut spit = vec![0; 36];
    put(&mut spit, 0, 10);
    put(&mut spit, 4, 1);
    put(&mut spit, 16, cast_type);
    put(&mut spit, 20, 0);
    let mut efit = 2.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&5_u32.to_le_bytes());
    let spell = Record::new(T(b"SPEL"), SPELL)
        .sub(T(b"SPIT"), spit)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500_u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(mgef);
    plugin.push_top(spell);
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 100.0)
        .unwrap();
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .spells
        .insert(id(SPELL));
    game.world_mut()
        .equip_hand_spell(id(ACTOR), Hand::Left, id(SPELL))
        .unwrap();
    game.world_mut()
        .equip_hand_spell(id(ACTOR), Hand::Right, id(SPELL))
        .unwrap();
    game
}

#[test]
fn mag_22_left_and_right_hands_accept_held_concentration_spells() {
    let mut game = fixture(2);
    let left = game.begin_hand_cast(id(ACTOR), Hand::Left, None);
    let right = game.begin_hand_cast(id(ACTOR), Hand::Right, None);

    assert!(matches!(left, Ok(true)), "left hand: {left:?}");
    assert!(matches!(right, Ok(true)), "right hand: {right:?}");
    for hand in [Hand::Left, Hand::Right] {
        assert!(!game.begin_hand_cast(id(ACTOR), hand, None).unwrap());
        assert!(game.end_hand_cast(id(ACTOR), hand));
        assert!(!game.world().is_concentration_casting(id(ACTOR)));
    }
}

#[test]
fn mag_22_releasing_hand_stops_world_concentration_cast() {
    let mut game = fixture(2);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .begin_concentration_cast(id(SPELL), id(ACTOR), id(ACTOR), None, &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    assert!(game.world().is_concentration_casting(id(ACTOR)));

    assert!(game.end_hand_cast(id(ACTOR), Hand::Left));

    assert!(!game.world().is_concentration_casting(id(ACTOR)));
}
