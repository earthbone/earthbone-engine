// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_05_navigation_to_current_position_succeeds_with_zero_tick_budget() {
    let mut game = crate::ply_02_routes::game(false);
    let player = game.player();
    let cell = crate::ply_02_routes::id(0x800);
    let position = [100.0, 0.0, 0.0];
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).expect("player reference");
        reference.cell = Some(cell);
        reference.position = position;
    }

    let report = crate::ply_02_routes::run(
        &mut game,
        "navigate-marker Skyrim.esm:800 100 0 0 120 0",
    );

    assert!(report.passed(), "{}", report.format());
    let player = game.world().reference_ref(player).expect("player reference");
    assert_eq!(player.position, position);
}
