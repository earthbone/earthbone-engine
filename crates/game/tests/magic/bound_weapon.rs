// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Bound Weapon grants, equips and expires one authored weapon item.

use super::*;
use soe_game::combat::Hand;

const BOUND_WEAPON: u32 = 0x700;
const RIGHT_SLOT: u32 = 0x701;
const LEFT_SLOT: u32 = 0x703;

fn bound_weapon_fixture() -> Game {
    let mut plugin = plugin(
        effect_with_associated_item(0, 17, 24, false, BOUND_WEAPON),
        vec![spell(SPELL, 0, false, 0.0, 10)],
    );
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&12_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 40, 0x2000);
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    plugin.push_top(
        Record::new(T(b"WEAP"), BOUND_WEAPON)
            .sub(T(b"DATA"), data)
            .sub(T(b"DNAM"), dnam)
            .sub(T(b"ETYP"), RIGHT_SLOT.to_le_bytes()),
    );
    let mut slots = u32::from_le_bytes(*b"LHEQ").to_le_bytes().to_vec();
    slots.extend_from_slice(&LEFT_SLOT.to_le_bytes());
    slots.extend_from_slice(&u32::from_le_bytes(*b"RHEQ").to_le_bytes());
    slots.extend_from_slice(&RIGHT_SLOT.to_le_bytes());
    plugin.push_top(Record::new(T(b"DOBJ"), 0x702).sub(T(b"DNAM"), slots));
    plugin.push_top(Record::new(T(b"EQUP"), RIGHT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"EQUP"), LEFT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()));
    game(plugin)
}

#[test]
fn mag_38_bound_weapon_grants_and_equips_the_authored_item() {
    let mut game = bound_weapon_fixture();

    assert_eq!(cast(&mut game, SPELL, ACTOR).unwrap().len(), 1);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);
    assert_eq!(
        game.world_mut().hand_weapon(id(ACTOR), Hand::Right),
        Some(id(BOUND_WEAPON))
    );
}

#[test]
fn mag_38_expiry_removes_the_effects_copy_and_clears_its_hand() {
    let mut game = bound_weapon_fixture();
    cast(&mut game, SPELL, ACTOR).unwrap();
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);

    game.world_mut()
        .tick_magic(10.0, &mut |_, _| soe_papyrus::ObjectId(999));

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 0);
    assert_eq!(game.world_mut().hand_weapon(id(ACTOR), Hand::Right), None);
    assert!(
        !game
            .world()
            .active_effects()
            .any(|effect| effect.effect == id(EFFECT))
    );
}

#[test]
fn mag_38_expiry_preserves_a_preexisting_weapon_copy_and_selection() {
    let mut game = bound_weapon_fixture();
    game.world_mut().add_item(id(ACTOR), id(BOUND_WEAPON), 1);
    cast(&mut game, SPELL, ACTOR).unwrap();
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 2);

    game.world_mut()
        .tick_magic(10.0, &mut |_, _| soe_papyrus::ObjectId(999));

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);
    assert_eq!(
        game.world_mut().hand_weapon(id(ACTOR), Hand::Right),
        Some(id(BOUND_WEAPON))
    );
}

#[test]
fn mag_38_installed_bound_effects_resolve_to_weapon_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-38 installed bound weapon survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut bound = 0;
    for (id, record) in order.winners_of(T(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect =
            soe_esm::records::gameplay::MagicEffect::from_record(record, localized).unwrap();
        if effect.data.archetype != 17 {
            continue;
        }
        let weapon = order.resolve(winner.file, effect.data.associated_item);
        let weapon_record = order.winner_ref(weapon).unwrap();
        assert_eq!(
            order.record(weapon_record).tag,
            T(b"WEAP"),
            "Bound Weapon MGEF {id:?} must link to a WEAP record"
        );
        bound += 1;
    }
    assert!(
        bound > 0,
        "five-master load order should contain Bound Weapon effects"
    );
}
