// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

#[test]
fn ply_05_city_soak_keeps_actors_on_their_walkable_surface() {
    let Some(mut game) = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA unset, skipping installed city soak");
        return;
    };

    for name in ["WhiterunExterior01", "Riverwood01"] {
        let cell = game.world_mut().cell_by_editor_id(name).expect("city cell");
        game.change_cell(Some(cell));
        let (graph, _) = soe_navmesh::vanilla::from_load_order(game.world().order(), |record| {
            record.placement.cell == Some(cell.object)
        });
        assert!(graph.triangle_count() > 0, "{name} has no walkable surface");
        let start_days = game.world().clock.game_days;
        for _ in 0..5_400 {
            game.tick(1.0 / 60.0);
            let world = game.world();
            for actor in world.loaded_actors() {
                let reference = world.reference_ref(actor).expect("loaded actor reference");
                assert!(reference.position.into_iter().all(f32::is_finite));
                let point = reference.position;
                if let Some(node) = graph.locate(point) {
                    let surface = graph.centroid(node)[2];
                    assert!(point[2] + 2.0 >= surface, "{name}: {actor:?} below NAVM");
                }
            }
        }
        let elapsed = (game.world().clock.game_days - start_days) * 86_400.0;
        assert!((elapsed - 1_800.0).abs() < 0.1, "{name}: {elapsed} seconds");
    }
}
