// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Installed-data checks for the PLY-05 city soak boundary.

const MASTERS: &[&str] = &[
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

fn installed_game() -> Option<soe_game::Game> {
    crate::common::installed::game(MASTERS)
}

#[test]
fn ply_05_adversarial_city_cells_have_placed_references_and_navmesh() {
    let Some(game) = installed_game() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };

    for name in ["WhiterunExterior01", "Riverwood01"] {
        let cell = game
            .world_mut()
            .cell_by_editor_id(name)
            .unwrap_or_else(|| panic!("missing installed city cell {name}"));
        let references = game.world_mut().cell_references(cell);
        assert!(
            references.iter().any(|id| {
                game.world()
                    .record(*id)
                    .is_some_and(|record| matches!(record.tag.as_bytes(), b"REFR" | b"ACHR"))
            }),
            "{name} resolves but has no placed objects"
        );

        let (graph, stats) = soe_navmesh::vanilla::from_load_order(game.world().order(), |record| {
            record.placement.cell == Some(cell.object)
        });
        assert!(
            stats.meshes > 0 && graph.triangle_count() > 0,
            "{name} has no cell-owned walkable NAVM geometry: {stats:?}"
        );
    }
}

#[test]
fn ply_05_adversarial_city_cell_navmesh_contains_a_reachable_route() {
    for name in ["WhiterunExterior01", "Riverwood01"] {
        let Some(game) = installed_game() else {
            eprintln!("skipping: SKYRIM_DATA is not set");
            return;
        };
        let cell = game
            .world_mut()
            .cell_by_editor_id(name)
            .unwrap_or_else(|| panic!("missing installed city cell {name}"));
        let (graph, _) = soe_navmesh::vanilla::from_load_order(game.world().order(), |record| {
            record.placement.cell == Some(cell.object)
        });

        let mut route = None;
        'meshes: for (mesh_index, mesh) in graph.meshes().iter().enumerate() {
            for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
                for neighbor in triangle.neighbors.iter().flatten() {
                    let from = soe_navmesh::NodeId::new(mesh_index as u32, triangle_index as u32);
                    if let Some(path) = graph.find_path(from, *neighbor) {
                        if path.nodes.len() > 1 && path.nodes.len() <= 8 {
                            route = Some((graph.centroid(from), graph.centroid(*neighbor)));
                            break 'meshes;
                        }
                    }
                }
            }
        }
        assert!(route.is_some(), "{name} has no short reachable NAVM route");
        let (from, to) = route.unwrap();
        assert!(from.into_iter().chain(to).all(f32::is_finite));
        assert_eq!(
            game.world()
                .record(cell)
                .map(|record| record.tag.as_bytes()),
            Some(b"CELL")
        );
    }
}
