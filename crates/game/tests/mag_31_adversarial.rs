// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::support::magic_adversarial as support;

use support::*;

#[test]
fn mag_31_frenzy_marker_and_aggression_survive_save_and_expire_together() {
    let magic_effect = effect(0x300, 8, 1 << 1, 0);
    let magic_spell = spell(0x301, 0x300, 2, 2.0, 1);
    let mut game = fixture(vec![magic_effect], vec![magic_spell], false);
    cast(&mut game, 0x301, TARGET);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "aggression"), 3.0);
    assert!(game.world().is_frenzied(id(TARGET)));

    let bytes = game.save_state().unwrap();
    let mut loaded = fixture(
        vec![effect(0x300, 8, 1 << 1, 0)],
        vec![spell(0x301, 0x300, 2, 2.0, 1)],
        false,
    );
    loaded.load_state(&bytes).unwrap();
    assert!(loaded.world().is_frenzied(id(TARGET)));
    loaded.tick(2.0);

    assert!(!loaded.world().is_frenzied(id(TARGET)));
    assert_eq!(
        loaded.world_mut().actor_value(id(TARGET), "aggression"),
        1.0
    );
}
