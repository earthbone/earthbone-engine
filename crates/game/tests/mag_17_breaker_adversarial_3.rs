// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::mag_adversarial::*;

#[test]
fn mag_17_location_cast_rejects_range_that_overflows_unit_conversion() {
    let mut game = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 4, f32::MAX, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0])],
    );

    let result = game.shared_world().borrow_mut().cast_spell_at_location(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        [0.0, 0.0, 0.0],
        true,
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        result.is_err(),
        "unrepresentable world-space range is invalid"
    );
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 100.0);
}
