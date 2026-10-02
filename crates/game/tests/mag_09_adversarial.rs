// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::magic::MagicError;

const ACTOR: u32 = 0x100;
const TARGET: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const PERK: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(manual_cost: Option<u32>, half_cost_perk: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for actor in [ACTOR, TARGET] {
        plugin.push_top(
            Record::new(Tag::new(b"NPC_"), actor + 0x1000).sub(Tag::new(b"ACBS"), vec![0; 24]),
        );
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), (actor + 0x1000).to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 0, 1 << 1);
    put(&mut effect_data, 16, u32::MAX);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);
    effect_data[4..8].copy_from_slice(&10.0f32.to_le_bytes());
    effect_data[48..52].copy_from_slice(&1.5f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), EFFECT).sub(Tag::new(b"DATA"), effect_data));
    let mut spit = vec![0; 36];
    put(&mut spit, 0, manual_cost.unwrap_or(0));
    put(&mut spit, 4, u32::from(manual_cost.is_some()));
    spit[12..16].copy_from_slice(&2.5f32.to_le_bytes());
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    if half_cost_perk {
        put(&mut spit, 32, PERK);
        plugin.push_top(Record::new(Tag::new(b"PERK"), PERK));
    }
    let mut efit = 2.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&5u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag::new(b"SPEL"), SPELL)
            .sub(Tag::new(b"SPIT"), spit)
            .sub(Tag::new(b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag::new(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Mag09.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ACTOR, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "magicka", 100.0)
            .unwrap();
    }
    if half_cost_perk {
        game.world_mut()
            .actor(id(ACTOR))
            .unwrap()
            .perks
            .insert(id(PERK));
    }
    game
}

#[test]
fn mag_09_each_successful_cast_charges_once_and_keeps_effect_cost_separate() {
    let mut game = fixture(None, false);
    for expected in [90.0, 80.0] {
        let shared = game.shared_world();
        shared
            .borrow_mut()
            .cast_charged_spell(
                id(SPELL),
                id(ACTOR),
                id(ACTOR),
                Some(id(TARGET)),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .unwrap();
        assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), expected);
    }
    assert_eq!(game.world().active_effects().count(), 2);
}

#[test]
fn mag_09_manual_cost_and_authored_charge_time_ignore_effect_formula() {
    let game = fixture(Some(37), false);
    assert_eq!(game.world().spell_charge_time(id(SPELL)).unwrap(), 2.5);
    assert_eq!(
        game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap(),
        37.0
    );
}

#[test]
fn mag_09_exact_magicka_succeeds_and_leaves_zero() {
    let mut game = fixture(Some(100), false);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 0.0);
}

#[test]
fn mag_09_half_cost_perk_changes_the_paid_amount() {
    let mut game = fixture(Some(100), true);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 50.0);
}

#[test]
fn mag_09_insufficient_magicka_rejects_without_payment_or_effect() {
    let mut game = fixture(Some(101), false);
    let shared = game.shared_world();
    assert!(matches!(
        shared.borrow_mut().cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        ),
        Err(MagicError::InsufficientMagicka)
    ));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_09_missing_target_rejects_without_debit_or_cast_event() {
    let mut game = fixture(None, false);
    let shared = game.shared_world();
    let error = shared
        .borrow_mut()
        .cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(0xdead)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap_err();
    assert!(matches!(error, MagicError::Target));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(!game.world().pending.iter().any(|pending|
        matches!(pending, soe_game::world::Pending::Event { name, .. } if name == "OnSpellCast")));
}
