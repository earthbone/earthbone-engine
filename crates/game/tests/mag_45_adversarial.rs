// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::support::magic_adversarial as support;

use support::*;

#[test]
fn mag_45_turn_undead_never_flees_or_changes_human_actor() {
    let magic_effect = effect(0x300, 24, 1 | (1 << 1), 0);
    let magic_spell = spell(0x301, 0x300, 2, 10.0, 1);
    let mut game = fixture(vec![magic_effect], vec![magic_spell], true);
    assert_eq!(cast(&mut game, 0x301, TARGET).len(), 1);
    assert!(cast(&mut game, 0x301, OTHER).is_empty());
    let before = game.world_mut().reference(id(OTHER)).unwrap().position;
    game.tick(0.1);

    assert_eq!(
        game.world_mut().reference(id(OTHER)).unwrap().position,
        before
    );
    assert_eq!(game.world().active_effects().count(), 1);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "confidence"), 1.0);
}
