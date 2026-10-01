// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::common::mag_adversarial as support;
use support::*;

#[test]
fn touch_reach_includes_its_boundary_and_rejects_the_next_distance() {
    let mut boundary = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 1, 128.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0]), (OTHER, [128.0, 0.0, 0.0])],
    );
    let applied = boundary.shared_world().borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(OTHER)),
        &mut |name, class| boundary.vm.create_object_of(name, class),
    );
    assert_eq!(applied.unwrap().len(), 1);
    assert_eq!(boundary.world_mut().actor_value(id(OTHER), "health"), 0.0);

    let mut beyond = game(
        effect(5, -1),
        vec![spell(SPELL, 1, 1, 128.0, 0)],
        &[(SOURCE, [0.0, 0.0, 0.0]), (OTHER, [128.01, 0.0, 0.0])],
    );
    let result = beyond.shared_world().borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(OTHER)),
        &mut |name, class| beyond.vm.create_object_of(name, class),
    );
    assert!(result.is_err());
    assert_eq!(beyond.world_mut().actor_value(id(OTHER), "health"), 100.0);
}
