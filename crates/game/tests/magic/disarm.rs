// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Disarm's level limit and loose weapon transfer.

use super::*;
use soe_game::combat::Hand;
use soe_game::world::DYNAMIC_FILE;

const WEAPON: u32 = 0x700;
const RIGHT_SLOT: u32 = 0x701;
const LEFT_SLOT: u32 = 0x703;

fn dropped_reference(game: &Game) -> Option<GlobalFormId> {
    (0x1000..0x1100)
        .map(|object| GlobalFormId {
            file: DYNAMIC_FILE,
            object,
        })
        .find(|reference| game.world().reference_ref(*reference).is_some())
}

fn disarm_fixture(magnitude: f32) -> Game {
    let mut plugin = plugin(
        effect(0, 9, 24, false),
        vec![spell(SPELL, 0, false, magnitude, 10)],
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
        Record::new(T(b"WEAP"), WEAPON)
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
fn mag_32_disarm_drops_one_equipped_weapon_as_a_recoverable_reference() {
    let mut game = disarm_fixture(5.0);
    game.world_mut().actor(id(ACTOR)).unwrap().level = 5;
    game.world_mut().add_item(id(ACTOR), id(WEAPON), 2);
    game.world_mut()
        .equip_hand_weapon(id(ACTOR), Hand::Right, id(WEAPON))
        .unwrap();
    {
        let mut world = game.world_mut();
        let actor = world.reference(id(ACTOR)).unwrap();
        actor.position = [12.0, 34.0, 56.0];
        actor.rotation = [0.1, 0.2, 0.3];
        actor.cell = Some(id(0x800));
    }

    cast(&mut game, SPELL, ACTOR).unwrap();

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(WEAPON)), 1);
    assert_eq!(game.world_mut().hand_weapon(id(ACTOR), Hand::Right), None);
    let dropped_id = dropped_reference(&game).unwrap();
    {
        let world = game.world();
        let dropped = world.reference_ref(dropped_id).unwrap();
        assert_eq!(dropped.base, Some(id(WEAPON)));
        assert_eq!(dropped.position, [12.0, 34.0, 56.0]);
        assert_eq!(dropped.rotation, [0.1, 0.2, 0.3]);
        assert_eq!(dropped.cell, Some(id(0x800)));
        assert!(dropped.enabled);
        assert_eq!(dropped.item.as_ref().map(|item| item.count()), Some(1));
    }

    assert!(
        game.world_mut()
            .add_item_notified(id(ACTOR), dropped_id, 1, &mut |_, _| soe_papyrus::ObjectId(
                999
            ),)
    );
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(WEAPON)), 2);
    assert!(
        game.world()
            .reference_ref(dropped_id)
            .unwrap()
            .item
            .is_none()
    );
    assert!(!game.world().reference_ref(dropped_id).unwrap().enabled);
}

#[test]
fn mag_32_disarm_respects_the_effect_magnitude_level_limit() {
    let mut game = disarm_fixture(4.0);
    game.world_mut().actor(id(ACTOR)).unwrap().level = 5;
    game.world_mut().add_item(id(ACTOR), id(WEAPON), 1);
    game.world_mut()
        .equip_hand_weapon(id(ACTOR), Hand::Right, id(WEAPON))
        .unwrap();

    cast(&mut game, SPELL, ACTOR).unwrap();

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(WEAPON)), 1);
    assert_eq!(
        game.world_mut().hand_weapon(id(ACTOR), Hand::Right),
        Some(id(WEAPON))
    );
    assert_eq!(dropped_reference(&game), None);
}

#[test]
fn mag_32_disarm_without_an_equipped_hand_weapon_does_not_drop_inventory() {
    let mut game = disarm_fixture(10.0);
    game.world_mut().add_item(id(ACTOR), id(WEAPON), 1);

    cast(&mut game, SPELL, ACTOR).unwrap();

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(WEAPON)), 1);
    assert_eq!(dropped_reference(&game), None);
}
