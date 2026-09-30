// SPDX-License-Identifier: GPL-3.0-or-later

//! Absorb effects transfer the selected actor value over their authored time.

use super::*;

#[test]
fn mag_28_absorb_transfers_primary_actor_value_at_magnitude_rate_for_duration() {
    let mut absorb = effect(1, 4, 25, false);
    let data = absorb
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"DATA"))
        .unwrap();
    put(&mut data.data, 88, 24);
    let mut game = game(plugin(absorb, vec![spell(SPELL, 0, false, 4.0, 3)]));
    game.world_mut()
        .damage_actor_value(id(OTHER), "magicka", 60.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(id(ACTOR), "magicka", 90.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick(1.5);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 34.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 16.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);

    game.tick(3.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 28.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 22.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_28_absorb_no_magnitude_flag_suppresses_both_sides_of_transfer() {
    let absorb = effect(1 << 10, 4, 25, false);
    let mut game = game(plugin(absorb, vec![spell(SPELL, 0, false, 12.0, 3)]));
    game.world_mut()
        .damage_actor_value(id(OTHER), "magicka", 60.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(id(ACTOR), "magicka", 90.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 40.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 10.0);
    game.tick(3.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 40.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 10.0);
}

#[test]
fn mag_28_absorb_no_duration_flag_applies_the_full_transfer_immediately() {
    let absorb = effect(1 << 9, 4, 25, false);
    let mut game = game(plugin(absorb, vec![spell(SPELL, 0, false, 12.0, 3)]));
    game.world_mut()
        .damage_actor_value(id(OTHER), "magicka", 60.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(id(ACTOR), "magicka", 90.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 28.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 22.0);
    assert_eq!(
        game.world().active_effects().next().unwrap().duration,
        Some(0.0)
    );
}
