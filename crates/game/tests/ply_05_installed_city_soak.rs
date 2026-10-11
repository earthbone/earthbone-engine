// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::HashMap;

use soe_esm::{
    records::{cell::Cell as CellRecord, land::Land},
    GlobalFormId, Tag,
};
use soe_game::{play, player_input::STEP_SECONDS};

const MASTERS: &[&str] = &[
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];
const CITIES: &[&str] = &["WhiterunExterior01", "Riverwood01"];

fn city_interiors(city: &str) -> &'static [&'static str] {
    match city {
        "WhiterunExterior01" => &["WhiterunBanneredMare", "WhiterunJorrvaskr"],
        "Riverwood01" => &["RiverwoodSleepingGiantInn", "RiverwoodTrader"],
        _ => &[],
    }
}

fn installed_game() -> Option<soe_game::Game> {
    let mut game = crate::common::installed::game(MASTERS)?;
    game.start();
    Some(game)
}

fn cell_grid(game: &soe_game::Game, cell: GlobalFormId) -> (i32, i32, GlobalFormId) {
    let world_ref = game.world();
    let record = world_ref.record(cell).expect("installed cell record");
    let parsed = CellRecord::from_record(record, false).expect("decode installed cell");
    let grid = parsed.grid.expect("exterior city cell grid");
    let source = world_ref.order().winner_ref(cell).expect("cell source");
    let world = world_ref.order().resolve(
        source.file,
        record.placement.world.expect("cell worldspace"),
    );
    (grid.x, grid.y, world)
}

fn terrain_for_cell(game: &mut soe_game::Game, cell: GlobalFormId) -> Vec<f32> {
    let land_id = game
        .world()
        .order()
        .build_cell_index()
        .land(cell)
        .unwrap_or_else(|| panic!("cell {cell} has no LAND record"));
    let world_ref = game.world();
    let record = world_ref.record(land_id).expect("winning LAND record");
    Land::from_record(record)
        .expect("decode LAND record")
        .heights
        .expect("LAND has heights")
        .decode()
}

fn land_height(heights: &[f32], x: f32, y: f32, grid_x: i32, grid_y: i32) -> Option<f32> {
    const SIDE: usize = 33;
    let local_x = (x - grid_x as f32 * 4096.0) / 128.0;
    let local_y = (y - grid_y as f32 * 4096.0) / 128.0;
    if !(0.0..=32.0).contains(&local_x) || !(0.0..=32.0).contains(&local_y) {
        return None;
    }
    let col = (local_x.floor() as usize).min(31);
    let row = (local_y.floor() as usize).min(31);
    let u = local_x - col as f32;
    let v = local_y - row as f32;
    let h00 = heights[row * SIDE + col];
    let h10 = heights[row * SIDE + col + 1];
    let h01 = heights[(row + 1) * SIDE + col];
    let h11 = heights[(row + 1) * SIDE + col + 1];
    Some((h00 * (1.0 - u) + h10 * u) * (1.0 - v) + (h01 * (1.0 - u) + h11 * u) * v)
}

fn navmesh_height(graph: &soe_navmesh::NavGraph, point: [f32; 3]) -> Option<f32> {
    let node = graph.locate(point)?;
    let mesh = graph.mesh(node.mesh);
    let triangle = graph.triangle(node);
    let [a, b, c] = triangle
        .vertices
        .map(|vertex| mesh.vertices[vertex as usize]);
    let ab: [f32; 3] = std::array::from_fn(|axis| b[axis] - a[axis]);
    let ac: [f32; 3] = std::array::from_fn(|axis| c[axis] - a[axis]);
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    (normal[2].abs() > 1e-6)
        .then(|| a[2] - (normal[0] * (point[0] - a[0]) + normal[1] * (point[1] - a[1])) / normal[2])
}

fn bot_opens_door(game: &mut soe_game::Game, target: GlobalFormId) -> bool {
    let (position, cell, locked, enabled, deleted, teleport) = {
        let mut world = game.world_mut();
        let Some(reference) = world.reference(target) else {
            return false;
        };
        (
            reference.position,
            reference.cell,
            reference.locked,
            reference.enabled,
            reference.deleted,
            reference.teleport.is_some(),
        )
    };
    if locked || !enabled || deleted {
        return false;
    }
    let Some(cell) = cell else {
        return false;
    };
    let player = game.player();
    {
        let mut world = game.world_mut();
        let state = world.reference(player).expect("player reference");
        state.cell = Some(cell);
        state.position = position;
    }
    game.change_cell(Some(cell));
    if game
        .apply_player_input(&soe_game::player_input::Command::Activate { target })
        .is_err()
    {
        return false;
    }
    let opened = game.world_mut().reference(target).is_some_and(|r| r.opened);
    let player = game.player();
    let traversed = teleport
        && game
            .world_mut()
            .reference(player)
            .is_some_and(|reference| reference.cell != Some(cell));
    opened || traversed
}

#[test]
fn ply_05_installed_city_bot_walks_each_city_and_checks_terrain_clearance() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA unset, skipping installed city bot soak");
        return;
    };
    let player = game.player();
    for name in CITIES {
        let cell = game.world_mut().cell_by_editor_id(name).expect("city cell");
        let (grid_x, grid_y, _) = cell_grid(&game, cell);
        let height_grid = terrain_for_cell(&mut game, cell);
        let (graph, _) = soe_navmesh::vanilla::from_load_order(game.world().order(), |record| {
            record.placement.cell == Some(cell.object)
        });
        assert!(graph.triangle_count() > 0, "{name} has no NAVM surface");
        let route = graph.meshes().iter().enumerate().find_map(|(mesh, value)| {
            value
                .triangles
                .iter()
                .enumerate()
                .find_map(|(triangle, data)| {
                    let from = soe_navmesh::NodeId::new(mesh as u32, triangle as u32);
                    let to = *data.neighbors.iter().flatten().next()?;
                    let path = graph.find_path(from, to)?;
                    (path.points.len() > 1)
                        .then(|| (graph.centroid(from), path.points.last().copied().unwrap()))
                })
        });
        let (from, to) = route.unwrap_or_else(|| panic!("{name} has no reachable walk route"));
        {
            let mut world = game.world_mut();
            let reference = world.reference(player).expect("player reference");
            reference.cell = Some(cell);
            reference.position = from;
        }
        game.change_cell(Some(cell));
        let route = play::Route::parse(&format!(
            "navigate-marker {name} {} {} {} 120 600\nexpect-no-vm-error",
            to[0], to[1], to[2]
        ))
        .expect("city route parses");
        let report = play::run(&mut game, &route);
        assert!(report.passed(), "{name}: {}", report.format());

        let ticks = (30.0 * 60.0 / 20.0 / STEP_SECONDS) as usize;
        let start_days = game.world().clock.game_days;
        let mut stuck_ticks = HashMap::<GlobalFormId, usize>::new();
        for _ in 0..ticks {
            game.tick(STEP_SECONDS);
            let world = game.world();
            for actor in world.loaded_actors() {
                let reference = world.reference_ref(actor).expect("loaded actor");
                assert!(reference.position.into_iter().all(f32::is_finite));
                assert!(reference.rotation.into_iter().all(f32::is_finite));
                if reference.cell == Some(cell) {
                    let surface = navmesh_height(&graph, reference.position).unwrap_or_else(|| {
                        panic!(
                            "{name}: actor {actor} is not over a NAVM surface at {:?}",
                            reference.position
                        )
                    });
                    assert!(
                        reference.position[2] + 2.0 >= surface,
                        "{name}: actor {actor} is below NAVM, z={} surface={surface}",
                        reference.position[2]
                    );
                    let surface = land_height(
                        &height_grid,
                        reference.position[0],
                        reference.position[1],
                        grid_x,
                        grid_y,
                    )
                    .unwrap_or_else(|| panic!("{name}: actor {actor} is outside its LAND cell"));
                    assert!(
                        reference.position[2] + 2.0 >= surface,
                        "{name}: actor {actor} is below LAND, z={} terrain={surface}",
                        reference.position[2]
                    );
                }
                let elapsed = stuck_ticks.entry(actor).or_default();
                if world
                    .ai
                    .metrics
                    .actors
                    .get(&actor)
                    .is_some_and(|metrics| metrics.stuck)
                {
                    *elapsed += 1;
                    assert!(
                        *elapsed <= 600,
                        "{name}: actor {actor} stayed stuck for more than ten real seconds"
                    );
                } else {
                    *elapsed = 0;
                }
            }
        }
        let elapsed = (game.world().clock.game_days - start_days) * 86_400.0;
        assert!(
            (elapsed - 1_800.0).abs() < 0.1,
            "{name}: {elapsed} game seconds"
        );
        assert!(
            !game
                .vm
                .log
                .entries
                .iter()
                .any(|entry| entry.message.starts_with("error:")),
            "{name}: Papyrus VM error"
        );
    }
}

#[test]
fn ply_05_installed_city_bot_opens_doors_activates_objects_and_starts_conversations() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA unset, skipping installed city interactions");
        return;
    };
    for name in CITIES {
        let cell = game.world_mut().cell_by_editor_id(name).expect("city cell");
        game.change_cell(Some(cell));
        let refs = game.world_mut().cell_references(cell);
        let mut opened_door = false;
        let mut activated_object = false;
        let mut conversation = false;
        for target in refs {
            let Some(record_tag) = game.world().record(target).map(|record| record.tag) else {
                continue;
            };
            if !matches!(record_tag.as_bytes(), b"REFR" | b"ACHR") {
                continue;
            }
            let Some(base) = game.world_mut().base_of(target) else {
                continue;
            };
            let Some(base_tag) = game.world().tag_of(base) else {
                continue;
            };
            let (enabled, deleted, locked, target_position, target_cell, is_actor) = {
                let mut world = game.world_mut();
                let target_state = world.reference(target).expect("placed reference");
                (
                    target_state.enabled,
                    target_state.deleted,
                    target_state.locked,
                    target_state.position,
                    target_state.cell,
                    target_state.is_actor,
                )
            };
            if !enabled || deleted || locked {
                continue;
            }
            {
                let player = game.player();
                let mut world = game.world_mut();
                let state = world.reference(player).expect("player reference");
                state.cell = target_cell;
                state.position = target_position;
            }
            game.change_cell(target_cell);
            if base_tag == Tag(*b"DOOR") && !opened_door {
                opened_door = bot_opens_door(&mut game, target);
            } else if matches!(base_tag.as_bytes(), b"ACTI" | b"CONT" | b"MISC")
                && !activated_object
                && game
                    .apply_player_input(&soe_game::player_input::Command::Activate { target })
                    .is_ok()
            {
                activated_object = true;
            }
            if is_actor && !conversation {
                let can_talk = game.world_mut().actor(target).is_some_and(|actor| {
                    !actor.dead && !actor.in_combat && !actor.unconscious && actor.allow_pc_dialogue
                });
                if !can_talk {
                    continue;
                }
                let command = soe_game::player_input::Command::Activate { target };
                if game.apply_player_input(&command).is_ok()
                    && game.world().dialogue_session().is_some()
                {
                    conversation = true;
                }
            }
            if opened_door && activated_object && conversation {
                break;
            }
        }
        if !(opened_door && activated_object && conversation) {
            for interior_name in city_interiors(name) {
                let Some(interior) = game.world_mut().cell_by_editor_id(interior_name) else {
                    continue;
                };
                let refs = game.world_mut().cell_references(interior);
                for target in refs {
                    let Some(record_tag) = game.world().record(target).map(|record| record.tag)
                    else {
                        continue;
                    };
                    if !matches!(record_tag.as_bytes(), b"ACHR" | b"REFR") {
                        continue;
                    }
                    let Some(base) = game.world_mut().base_of(target) else {
                        continue;
                    };
                    let Some(base_tag) = game.world().tag_of(base) else {
                        continue;
                    };
                    let (position, target_cell, enabled) = {
                        let mut world = game.world_mut();
                        let Some(reference) = world.reference(target) else {
                            continue;
                        };
                        (
                            reference.position,
                            reference.cell,
                            reference.enabled && !reference.deleted,
                        )
                    };
                    if !enabled {
                        continue;
                    }
                    let Some(target_cell) = target_cell else {
                        continue;
                    };
                    let player = game.player();
                    {
                        let mut world = game.world_mut();
                        let state = world.reference(player).expect("player reference");
                        state.cell = Some(target_cell);
                        state.position = position;
                    }
                    game.change_cell(Some(target_cell));
                    match base_tag {
                        tag if tag == Tag(*b"DOOR") && !opened_door => {
                            opened_door = bot_opens_door(&mut game, target);
                        }
                        tag if matches!(tag.as_bytes(), b"ACTI" | b"CONT" | b"MISC")
                            && !activated_object =>
                        {
                            activated_object = game
                                .apply_player_input(&soe_game::player_input::Command::Activate {
                                    target,
                                })
                                .is_ok();
                        }
                        _ if record_tag == Tag(*b"ACHR") && !conversation => {
                            if game
                                .apply_player_input(&soe_game::player_input::Command::Activate {
                                    target,
                                })
                                .is_ok()
                                && game.world().dialogue_session().is_some()
                            {
                                conversation = true;
                            }
                        }
                        _ => {}
                    }
                    if opened_door && activated_object && conversation {
                        break;
                    }
                }
                if opened_door && activated_object && conversation {
                    break;
                }
            }
        }
        assert!(
            opened_door,
            "{name}: no installed door was opened by the bot"
        );
        assert!(
            activated_object,
            "{name}: no installed object was activated by the bot"
        );
        assert!(
            conversation,
            "{name}: no installed actor conversation was started"
        );
    }
}
