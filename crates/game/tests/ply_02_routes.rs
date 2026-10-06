// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PLY-02 authored routes exercise shared gameplay, NAVM and corpus startup.

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{Entry, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::play::{self, Route};
use soe_game::player_input::STEP_SECONDS;
use soe_game::Game;

pub(super) fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn square(cell: u32, disconnected: bool) -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(cell),
        vertices: vec![
            [-10.0, -10.0, 0.0],
            [200.0, -10.0, 0.0],
            [200.0, 200.0, 0.0],
            [-10.0, 200.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, if disconnected { -1 } else { 1 }],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [if disconnected { -1 } else { 0 }, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [210.0, 210.0],
            min: [-10.0, -10.0, 0.0],
            max: [200.0, 200.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn insert_nav(groups: &mut [Group], cell: u32, index: usize) {
    for group in groups {
        if group.kind == GroupKind::CellTemporary(cell) {
            group.entries.push(Entry::Record(index));
            return;
        }
        for entry in &mut group.entries {
            if let Entry::Group(group) = entry {
                insert_nav(std::slice::from_mut(group), cell, index);
            }
        }
    }
}

pub(super) fn game(disconnected: bool) -> Game {
    game_with(disconnected, |_| {})
}

pub(super) fn game_with(disconnected: bool, edit: impl FnOnce(&mut Plugin)) -> Game {
    let mut plugin = super::ply_01_player_input::plugin();
    for record in &mut plugin.records {
        if record.form_id == 0x840 {
            *record = soe_game::quest::with_stage(record.clone(), 0, 0, 0, "Route start");
            *record = soe_game::quest::with_stage(record.clone(), 10, 0, 0, "Route checkpoint");
        }
    }
    for cell in [0x800, 0x801] {
        let index = plugin.push_record(
            Record::new(Tag(*b"NAVM"), cell + 0x1000)
                .sub(Tag(*b"NVNM"), square(cell, disconnected).to_bytes()),
        );
        insert_nav(&mut plugin.groups, cell, index);
    }
    let mut plugin = Plugin::parse(&plugin.to_bytes()).unwrap();
    edit(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order.build_cell_index();
    let mut game = Game::new(order);
    for (_, script) in super::ply_01_player_input::scripts() {
        game.load_script(&script).unwrap();
    }
    game.start();
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(id(0x800));
    game.change_cell(Some(id(0x800)));
    game
}

pub(super) fn ply_05_soak(seed: u64) -> String {
    let mut game = game(false);
    let mut state = if seed == 0 { 1 } else { seed };
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    let movement_rate = 80 + state % 41;
    let route = format!(
        "navigate Skyrim.esm:810 {movement_rate} 120\n\
         activate Skyrim.esm:810\n\
         activate Skyrim.esm:811\n\
         navigate-marker Skyrim.esm:801 105 0 0 {movement_rate} 120\n\
         navigate Skyrim.esm:820 {movement_rate} 120\n\
         activate Skyrim.esm:820\n\
         navigate Skyrim.esm:830 {movement_rate} 120\n\
         talk Skyrim.esm:830 Skyrim.esm:841\n\
         expect-item player Skyrim.esm:821 1\n\
         expect-no-vm-error\n"
    );
    let report = run(&mut game, &route);
    assert!(report.passed(), "{}", report.format());

    // At timescale 20, 5,400 fixed steps are 30 minutes of game time.
    let ticks = (30.0 * 60.0 / 20.0 / STEP_SECONDS) as usize;
    let start_days = game.world().clock.game_days;
    for _ in 0..ticks {
        game.tick(STEP_SECONDS);
        {
            let world = game.world();
            let player = world.reference_ref(id(0x14)).unwrap();
            assert!(player.position.into_iter().all(f32::is_finite));
            assert!(player.rotation.into_iter().all(f32::is_finite));
            for actor in world.loaded_actors() {
                let reference = world.reference_ref(actor).unwrap();
                assert!(reference.position.into_iter().all(f32::is_finite));
                assert!(reference.rotation.into_iter().all(f32::is_finite));
                assert!(
                    reference.position[2] >= 0.0,
                    "actor {actor} fell below fixture NAVM"
                );
            }
        }
    }
    let elapsed_game_seconds = (game.world().clock.game_days - start_days) * 86_400.0;
    assert!((elapsed_game_seconds - 1_800.0).abs() < 0.02);
    assert!(!game
        .vm
        .log
        .entries
        .iter()
        .any(|entry| entry.message.starts_with("error:")));
    format!("seed={seed}\n{}ticks={ticks}\n", report.format())
}

#[test]
fn ply_05_seeded_bot_walks_interacts_checks_invariants_and_repeats_its_report() {
    let seed = 0x5EED_0501;
    let first = ply_05_soak(seed);
    let second = ply_05_soak(seed);
    assert_eq!(first, second);
    assert!(first.contains("ticks=5400"));
    assert!(first.contains("completed="));
}

#[test]
fn ply_05_installed_city_cells_are_available_for_headless_soak() {
    use crate::common::installed;

    let Some(game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    for cell_name in ["WhiterunExterior01", "Riverwood01"] {
        let cell = game
            .world_mut()
            .cell_by_editor_id(cell_name)
            .unwrap_or_else(|| panic!("installed Skyrim cell {cell_name} is missing"));
        assert!(game.world().record(cell).is_some());
    }
}

pub(super) fn run(game: &mut Game, text: &str) -> play::Report {
    play::run(game, &Route::parse(text).unwrap())
}

#[test]
fn ply_02_route_navigates_activates_talks_and_checks_world_and_vm() {
    let mut game = game(false);
    let report = run(
        &mut game,
        "# authored route\n\
        navigate Skyrim.esm:810 120 120\n\
        activate Skyrim.esm:810\n\
        activate Skyrim.esm:811\n\
        navigate-marker Skyrim.esm:801 105 0 0 120 120\n\
        navigate Skyrim.esm:820 120 120\n\
        activate Skyrim.esm:820\n\
        expect-item player Skyrim.esm:821 1\n\
        navigate Skyrim.esm:830 120 120\n\
        talk Skyrim.esm:830 Skyrim.esm:841\n\
        expect-stage PlyQuest 0\n\
        expect-position Skyrim.esm:801 120 0 0 0.01\n\
        expect-no-vm-error",
    );
    assert!(report.passed(), "{}", report.format());
    assert!(game.world().reference_ref(id(0x810)).unwrap().opened);
    assert!(
        report.trace.iter().any(|line| line.contains("topic end")),
        "{}",
        report.format()
    );
}

#[test]
fn ply_02_navigation_consumes_fixed_steps_and_reports_a_timeout() {
    let mut game = game(false);
    let report = run(&mut game, "navigate-marker Skyrim.esm:800 100 0 0 60 3");
    assert!(!report.passed());
    assert_eq!(report.ticks, 3);
    assert_eq!(
        game.world().reference_ref(id(0x14)).unwrap().position,
        [3.0, 0.0, 0.0]
    );
    assert!(report.format().contains("FAIL step 1 line 1"));
    assert!(report.format().contains("world state:"));
    assert!(report.format().contains("trace:"));
    assert!(report.format().contains("position [3.0, 0.0, 0.0]"));
}

#[test]
fn ply_02_navigation_rejects_disconnected_off_mesh_and_foreign_cell_targets() {
    for route in [
        "navigate-marker Skyrim.esm:800 0 100 0 120 100",
        "navigate-marker Skyrim.esm:800 1000 1000 0 120 100",
        "navigate-marker Skyrim.esm:801 100 0 0 120 100",
    ] {
        let mut game = game(true);
        let report = run(&mut game, route);
        assert!(!report.passed(), "{route}");
        assert_eq!(report.ticks, 0, "{}", report.format());
        assert_eq!(
            game.world().reference_ref(id(0x14)).unwrap().position,
            [0.0; 3]
        );
    }
}

#[test]
fn ply_02_wait_stage_pumps_latent_scripts_without_setting_the_stage() {
    use soe_papyrus::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
    let mut game = game(false);
    let bytes = ScriptBuilder::new("RouteWait", "Quest")
        .function(
            FunctionBuilder::new("OnRoute", "None")
                .local("unused", "None")
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("Wait"), ident("unused"), f(0.05)],
                )
                .ins(
                    CallMethod,
                    vec![s("SetStage"), ident("self"), ident("unused"), i(10)],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(id(0x840));
    game.vm.attach(object, "RouteWait").unwrap();
    game.send_event(id(0x840), "OnRoute", &[]);
    let report = run(
        &mut game,
        "wait-stage PlyQuest 10 20\nexpect-stage PlyQuest 10\nexpect-no-vm-error",
    );
    assert!(report.passed(), "{}", report.format());
    assert!(report.ticks >= 3 && report.ticks <= 20);
}

#[test]
fn ply_02_wait_stage_times_out_and_does_not_treat_a_higher_stage_as_done() {
    let mut game = game(false);
    game.world_mut().quest_state(id(0x840)).unwrap().highest = 20;
    let report = run(&mut game, "wait-stage PlyQuest 10 2");
    assert!(!report.passed());
    assert_eq!(report.ticks, 2);
    assert!(report.failure.unwrap().contains("timeout waiting"));
}

#[test]
fn ply_02_assertions_fail_on_wrong_stage_position_cell_count_and_vm_error() {
    for route in [
        "expect-stage PlyQuest 10",
        "expect-position Skyrim.esm:800 1 0 0 0.5",
        "expect-position Skyrim.esm:801 0 0 0 1",
        "expect-item player Skyrim.esm:821 1",
        "expect-no-vm-error",
        "activate Skyrim.esm:999999",
        "activate Skyrim.esm:10000810",
    ] {
        let mut game = game(false);
        game.vm.log.push(
            game.vm.time(),
            "error: failed native (RouteError.OnRoute instruction 0)",
        );
        let report = run(&mut game, route);
        assert!(!report.passed(), "{route}");
        assert_eq!(report.completed, 0);
        assert!(report.state.contains("player"));
    }
}

#[test]
fn ply_02_parser_preserves_lines_quotes_and_rejects_malformed_input() {
    let route =
        Route::parse("# comment\nactivate \"My Plugin.esp:800\" # comment\nexpect-no-vm-error")
            .unwrap();
    assert_eq!(
        route.steps[0],
        (2, play::Step::Activate("My Plugin.esp:800".into()))
    );
    for text in [
        "",
        "activate",
        "talk a",
        "activate \"\"",
        "activate \"unterminated",
        "expect-stage a -1",
        "expect-position a NaN 0 0 1",
        "expect-position a 0 0 0 -1",
        "navigate a 0 10",
        "navigate a 601 10",
        "wait-stage a 1 36001",
        "expect-item a b nope",
        "expect-no-vm-error extra",
    ] {
        assert!(Route::parse(text).is_err(), "{text}");
    }
}

#[test]
fn ply_02_identical_routes_produce_identical_failure_reports() {
    let text = "navigate-marker Skyrim.esm:800 100 0 0 60 3";
    assert_eq!(run(&mut game(false), text), run(&mut game(false), text));
}

#[test]
fn ply_02_blocked_activation_keeps_on_activate_trace_and_stops_the_route() {
    let mut game = game(false);
    game.world_mut()
        .reference(id(0x810))
        .unwrap()
        .activation_blocked = true;
    let report = run(
        &mut game,
        "activate Skyrim.esm:810\nactivate Skyrim.esm:811",
    );
    assert!(!report.passed());
    assert_eq!(report.ticks, 1);
    assert!(report
        .trace
        .iter()
        .any(|line| line.contains("door activated")));
    assert!(!game.world().reference_ref(id(0x810)).unwrap().opened);
    assert_eq!(game.world().loaded_cell(), Some(id(0x800)));
}

#[test]
fn ply_02_closed_navm_door_blocks_movement_until_activated() {
    use soe_esm::records::navm::DoorLink;
    let mut game = game_with(false, |plugin| {
        let record = plugin
            .records
            .iter_mut()
            .find(|r| r.form_id == 0x1800)
            .unwrap();
        let data = &mut record
            .subrecords
            .iter_mut()
            .find(|r| r.tag == Tag(*b"NVNM"))
            .unwrap()
            .data;
        let mut nav = Geometry::parse(Tag(*b"NVNM"), data).unwrap();
        nav.door_links.push(DoorLink {
            triangle: 0,
            crc: 0,
            door: 0x810,
        });
        *data = nav.to_bytes();
    });
    let report = run(&mut game, "navigate-marker Skyrim.esm:800 100 0 0 120 120");
    assert!(!report.passed());
    assert_eq!(report.ticks, 0);
    let report = run(
        &mut game,
        "activate Skyrim.esm:810\nnavigate-marker Skyrim.esm:800 100 0 0 120 120",
    );
    assert!(report.passed(), "{}", report.format());
}

#[test]
fn ply_02_navigation_follows_corners_without_cutting_off_the_navmesh() {
    let mut game = game_with(false, |plugin| {
        let vertices = vec![
            [0., 0., 0.],
            [100., 0., 0.],
            [100., 100., 0.],
            [200., 100., 0.],
            [200., 200., 0.],
            [0., 200., 0.],
        ];
        let mut graph = soe_navmesh::NavGraph::new();
        graph.add_mesh_with_adjacency(
            vertices.clone(),
            &[[0, 1, 2], [0, 2, 5], [2, 4, 5], [2, 3, 4]],
        );
        let mut nav = square(0x800, false);
        nav.vertices = vertices;
        nav.triangles = graph
            .mesh(0)
            .triangles
            .iter()
            .map(|triangle| Triangle {
                vertices: triangle.vertices.map(|v| v as u16),
                edges: triangle
                    .neighbors
                    .map(|n| n.map_or(-1, |n| n.triangle as i16)),
                flags: 0,
                cover_flags: 0,
            })
            .collect();
        let record = plugin
            .records
            .iter_mut()
            .find(|r| r.form_id == 0x1800)
            .unwrap();
        record
            .subrecords
            .iter_mut()
            .find(|r| r.tag == Tag(*b"NVNM"))
            .unwrap()
            .data = nav.to_bytes();
    });
    let report = run(&mut game, "navigate-marker Skyrim.esm:800 190 110 0 600 100\nexpect-position Skyrim.esm:800 190 110 0 0.01");
    assert!(report.passed(), "{}", report.format());
    assert!(
        report
            .trace
            .iter()
            .any(|line| line.contains("position [100.0, 100.0, 0.0]")),
        "{}",
        report.format()
    );
}

#[test]
fn ply_02_navigation_retains_waypoint_cell_ownership_across_an_exterior_seam() {
    use soe_esm::records::navm::EdgeLink;
    let mut game = game_with(false, |plugin| {
        plugin.push_top(Record::new(Tag(*b"WRLD"), 0x700));
        for record in &mut plugin.records {
            if record.tag == Tag(*b"CELL") {
                record.placement.world = Some(0x700);
                record
                    .subrecords
                    .iter_mut()
                    .find(|r| r.tag == Tag(*b"DATA"))
                    .unwrap()
                    .data = vec![0, 0];
            }
            if record.tag == Tag(*b"NAVM") {
                let data = &mut record
                    .subrecords
                    .iter_mut()
                    .find(|r| r.tag == Tag(*b"NVNM"))
                    .unwrap()
                    .data;
                let mut nav = Geometry::parse(Tag(*b"NVNM"), data).unwrap();
                if record.form_id == 0x1800 {
                    nav.triangles[0].edges[1] = 0;
                    nav.triangles[0].flags = 2;
                    nav.edge_links.push(EdgeLink {
                        kind: 0,
                        navmesh: 0x1801,
                        triangle: 1,
                    });
                } else {
                    for vertex in &mut nav.vertices {
                        vertex[0] += 210.0;
                    }
                    nav.grid.min[0] += 210.0;
                    nav.grid.max[0] += 210.0;
                    nav.triangles[1].edges[2] = 0;
                    nav.triangles[1].flags = 4;
                    nav.edge_links.push(EdgeLink {
                        kind: 0,
                        navmesh: 0x1800,
                        triangle: 0,
                    });
                }
                *data = nav.to_bytes();
            }
        }
    });
    let report = run(&mut game, "navigate-marker Skyrim.esm:801 210 0 0 600 100\nexpect-position Skyrim.esm:801 210 0 0 0.01");
    assert!(report.passed(), "{}", report.format());
    assert!(
        report
            .trace
            .iter()
            .any(|line| line.contains("cell 0:000801 position [200.0,")),
        "{}",
        report.format()
    );
}

#[test]
fn ply_02_soe_play_runs_a_corpus_save_headless_and_reports_failure_status() {
    let (Some(data), Some(corpus)) = (
        crate::common::installed::data_dir(),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        return;
    };
    let save = std::path::PathBuf::from(corpus)
        .join("Main Quest + Dawnguard(Vampire) + Dragonborn/01 Right Off The Block.ess");
    let mut game = play::load_world_checkpoint(&data, &save).unwrap();
    let quest = game.world().form("MQ101").unwrap();
    let ess = soe_save::Save::open(&save).unwrap();
    let stage = {
        let world = game.world();
        let mapping = soe_game::vanilla_save::SavedFormMap::new(&ess, world.order()).unwrap();
        let change = ess
            .change_forms
            .iter()
            .find(|change| mapping.reference(change.form).unwrap() == Some(quest))
            .unwrap();
        change
            .quest_change()
            .unwrap()
            .unwrap()
            .stages
            .unwrap()
            .entries
            .iter()
            .filter(|stage| stage.status == 1)
            .map(|stage| stage.stage)
            .max()
            .unwrap_or(0)
    };
    let player = game.player();
    let world = game.world();
    let reference = world.reference_ref(player).unwrap();
    let cell = reference.cell.unwrap();
    assert_eq!(cell.file, 0);
    let (graph, _) = soe_navmesh::vanilla::from_load_order(world.order(), |record| {
        record.placement.cell == Some(cell.object)
    });
    let node = graph
        .locate(reference.position)
        .expect("corpus starting point has a walkable NAVM footprint");
    let point = graph.centroid(node);
    let gold = world.form("Gold001").unwrap();
    let count = reference.inventory.count(gold);
    drop(world);
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ply-02-routes");
    std::fs::create_dir_all(&root).unwrap();
    let route = root.join("corpus.route");
    std::fs::write(
        &route,
        format!("expect-stage MQ101 {stage}\nexpect-item player Gold001 {count}\nnavigate-marker Skyrim.esm:{:x} {} {} {} 120 3600\nexpect-position Skyrim.esm:{:x} {} {} {} 0.25\nexpect-no-vm-error\n", cell.object, point[0],point[1],point[2],cell.object,point[0],point[1],point[2]),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-play"))
        .args([data.as_os_str(), save.as_os_str(), route.as_os_str()])
        .arg("--world-checkpoint")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("WORLD CHECKPOINT"));
    std::fs::write(&route, "expect-stage MQ101 65535\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-play"))
        .args([data.as_os_str(), save.as_os_str(), route.as_os_str()])
        .arg("--world-checkpoint")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report = String::from_utf8(output.stdout).unwrap();
    assert!(report.contains("FAIL step 1 line 1"));
    assert!(report.contains("world state:"));
    assert!(report.contains("trace:"));
}

#[test]
fn ply_02_item_assertions_follow_shared_reference_list_and_keyword_queries() {
    let mut game = game_with(false, |plugin| {
        plugin.push_top(Record::new(Tag(*b"KYWD"), 0x1900));
        plugin.push_top(
            Record::new(Tag(*b"FLST"), 0x1901).sub(Tag(*b"LNAM"), 0x821u32.to_le_bytes()),
        );
        let item = plugin
            .records
            .iter_mut()
            .find(|record| record.form_id == 0x821)
            .unwrap();
        item.subrecords.push(soe_esm::Subrecord::new(
            Tag(*b"KWDA"),
            0x1900u32.to_le_bytes(),
        ));
    });
    let report = run(&mut game, "activate Skyrim.esm:811\nactivate Skyrim.esm:820\nexpect-item player Skyrim.esm:821 1\nexpect-item player Skyrim.esm:820 1\nexpect-item player Skyrim.esm:1901 1\nexpect-item player Skyrim.esm:1900 1");
    assert!(report.passed(), "{}", report.format());
}

#[test]
fn ply_02_vm_error_assertion_reports_a_real_failed_script_and_its_context() {
    use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallStatic, Return};
    let mut game = game(false);
    let bytes = ScriptBuilder::new("BrokenRoute", "Quest")
        .function(
            FunctionBuilder::new("OnRoute", "None")
                .local("unused", "None")
                .ins(
                    CallStatic,
                    vec![s("MissingClass"), s("MissingFunction"), ident("unused")],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(id(0x840));
    game.vm.attach(object, "BrokenRoute").unwrap();
    game.send_event(id(0x840), "OnRoute", &[]);
    let report = run(&mut game, "expect-no-vm-error");
    assert!(!report.passed());
    assert!(report.failure.unwrap().contains("BrokenRoute.OnRoute"));
    assert!(report
        .trace
        .iter()
        .any(|line| line.contains("MissingFunction")));
}

#[test]
fn ply_02_binary_rejects_invalid_arguments_and_malformed_routes_before_startup() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-play"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ply-02-routes");
    std::fs::create_dir_all(&root).unwrap();
    let route = root.join("malformed.route");
    std::fs::write(&route, "navigate target NaN 100\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-play"))
        .args(["missing-data", "missing-save.ess"])
        .arg(&route)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("route line 1"));
}

#[test]
fn ply_02_navigation_fails_when_a_latent_script_moves_the_player() {
    use soe_papyrus::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
    let mut game = game(false);
    let bytes = ScriptBuilder::new("InterruptRoute", "Quest")
        .function(
            FunctionBuilder::new("OnRoute", "None")
                .local("unused", "None")
                .local("player", "Actor")
                .local("target", "ObjectReference")
                .ins(
                    CallStatic,
                    vec![s("Utility"), s("Wait"), ident("unused"), f(0.05)],
                )
                .ins(CallStatic, vec![s("Game"), s("GetPlayer"), ident("player")])
                .ins(
                    CallStatic,
                    vec![
                        s("Game"),
                        s("GetFormFromFile"),
                        ident("target"),
                        i(0x820),
                        s("Skyrim.esm"),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("MoveTo"),
                        ident("player"),
                        ident("unused"),
                        ident("target"),
                    ],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(id(0x840));
    game.vm.attach(object, "InterruptRoute").unwrap();
    game.send_event(id(0x840), "OnRoute", &[]);
    let report = run(&mut game, "navigate-marker Skyrim.esm:800 100 0 0 60 200");
    assert!(!report.passed());
    assert!(report.failure.unwrap().contains("interrupted"));
    assert_eq!(
        game.world().reference_ref(id(0x14)).unwrap().cell,
        Some(id(0x801))
    );
}

#[path = "ply_02_adversarial.rs"]
mod adversarial;
