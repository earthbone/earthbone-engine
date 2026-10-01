// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../game/tests/common/mag_adversarial.rs"]
mod common;

use common::{effect, game, id, spell, OTHER, SOURCE, SPELL};
use soe_godot_host::combat::AimedProjectileContacts;

#[test]
fn mag_02_projectile_contact_applies_aimed_hostile_spell_once() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 2, 100.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0]), (OTHER, [10.0, 0.0, 0.0])],
    );
    let mut contacts = AimedProjectileContacts::default();
    let mut applied = 0;
    for _ in 0..2 {
        contacts.contact(55, || {
            let shared = game.shared_world();
            let result = shared.borrow_mut().cast_spell_on_projectile_contact(
                id(SPELL),
                id(SOURCE),
                Some(id(SOURCE)),
                id(OTHER),
                &mut |name, class| game.vm.create_object_of(name, class),
            );
            if result.is_ok() {
                applied += 1;
                true
            } else {
                false
            }
        });
    }
    assert_eq!(applied, 1);
    assert_eq!(game.world().active_effects().count(), 1);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 0.0);
}
