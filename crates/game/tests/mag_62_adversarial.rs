#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::mag_adversarial as common;
use common::{cast, effect, game, id, spell, ACTOR, OTHER, SPELL};

#[test]
fn mag_62_etherealize_restriction_survives_save_load_and_ends_with_effect() {
    let mut mgef = effect(0, -1);
    mgef.subrecords[0].data[64..68].copy_from_slice(&41_u32.to_le_bytes());
    let mut original = game(mgef, vec![spell(SPELL, 1, 3, 0.0, 0)], &[]);
    cast(&mut original, SPELL, ACTOR, OTHER).unwrap();
    let saved = original.save_state().unwrap();
    let mut mgef = effect(0, -1);
    mgef.subrecords[0].data[64..68].copy_from_slice(&41_u32.to_le_bytes());
    let mut restored = game(mgef, vec![spell(SPELL, 1, 3, 0.0, 0)], &[]);
    restored.load_state(&saved).unwrap();
    assert!(restored.world().is_ethereal(id(OTHER)));
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(10.0);
    assert!(!restored.world().is_ethereal(id(OTHER)));
    assert_eq!(restored.world().active_effects().count(), 0);
}
