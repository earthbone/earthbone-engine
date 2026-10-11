// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_05_disconnected_navmesh_route_fails_without_moving_the_player() {
    let mut game = crate::ply_02_routes::game(true);
    let player = game.player();
    let start = [100.0, 0.0, 0.0];
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).expect("player reference");
        reference.cell = Some(crate::ply_02_routes::id(0x800));
        reference.position = start;
    }

    let report =
        crate::ply_02_routes::run(&mut game, "navigate-marker Skyrim.esm:800 0 100 0 120 120");

    assert!(!report.passed(), "disconnected NAVM unexpectedly passed");
    assert!(
        report
            .failure
            .as_deref()
            .is_some_and(|failure| failure.contains("no walkable NAVM path")),
        "{}",
        report.format()
    );
    let moved = game.world().reference_ref(player).expect("player");
    assert_eq!(moved.position, start);
    assert!(moved.position.into_iter().all(f32::is_finite));
}
