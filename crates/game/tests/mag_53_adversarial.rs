// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Accumulate Magnitude uses the MGEF primary actor value and unpaused time.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const ACTOR: u32 = 0x100;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut mgef = vec![0; 152];
    put(&mut mgef, 0, 1 << 9); // No Duration.
    put(&mut mgef, 8, 0); // No associated item.
    put(&mut mgef, 16, u32::MAX);
    put(&mut mgef, 64, 32); // Accumulate Magnitude.
    put(&mut mgef, 68, 63); // PrimaryAV: WardPower.
    put(&mut mgef, 84, 3); // Target Actor delivery.

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    put(&mut spell_data, 16, 0);
    put(&mut spell_data, 20, 3);
    let mut efit = 40.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&10u32.to_le_bytes());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x500u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), mgef));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let setting = 20.0f32.to_le_bytes().to_vec();
    plugin.push_top(
        Record::new(T(b"GMST"), 0x400)
            .zsub(T(b"EDID"), "fMagicWardPowerMaxBase")
            .sub(T(b"DATA"), setting),
    );
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game
}

fn cast(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_53_accumulates_primary_actor_value_on_unpaused_clock_and_caps_at_magnitude() {
    let mut game = fixture();
    cast(&mut game);
    game.tick_menu_mode(4.0, true);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "wardpower"), 0.0);
    game.tick(1.5);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "wardpower"), 30.0);
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "wardpower"), 40.0);
}

#[test]
fn mag_53_accumulated_amount_and_effect_resume_after_reload() {
    let mut original = fixture();
    cast(&mut original);
    original.tick(0.75);
    let saved = original.save_state().unwrap();

    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world_mut().actor_value(id(ACTOR), "wardpower"),
        15.0
    );
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(0.5);
    assert_eq!(
        restored.world_mut().actor_value(id(ACTOR), "wardpower"),
        25.0
    );
}

#[test]
fn mag_53_accumulate_remains_capped_across_large_ticks() {
    let mut game = fixture();
    cast(&mut game);
    game.tick(100.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "wardpower"), 40.0);
    assert_eq!(game.world().active_effects().count(), 1);
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "wardpower"), 40.0);
}
