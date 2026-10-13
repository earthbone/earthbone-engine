// SPDX-License-Identifier: GPL-3.0-or-later

//! Debug geometry contracts against authored NAVM and live routes. Counts are
//! fixed before execution, with remote interiors excluded from the rendered view.

use super::*;
use soe_esm::records::navm::{DoorLink, EdgeLink, NavGrid, Parent, Triangle};
use soe_esm::{FileHeader, LoadOrder, Plugin, Record};
use soe_game::{debug::Console, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn geometry() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(0x900),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [0.0, 100.0, 0.0],
            [100.0, 100.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [0, 1, -1],
                flags: 0x201,
                cover_flags: 2,
            },
            Triangle {
                vertices: [1, 3, 2],
                edges: [-1, -1, 0],
                flags: 0x440,
                cover_flags: 4 << 6,
            },
        ],
        edge_links: vec![EdgeLink {
            kind: 0,
            navmesh: 0x911,
            triangle: 0,
        }],
        door_links: vec![DoorLink {
            triangle: 1,
            crc: 0,
            door: 0x830,
        }],
        cover_triangles: vec![0, 1],
        grid: NavGrid {
            divisor: 1,
            size: [100.0; 2],
            min: [0.0; 3],
            max: [100.0, 100.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for cell in [0x900, 0x901] {
        plugin.push_top(Record::new(Tag(*b"CELL"), cell).sub(Tag(*b"DATA"), [1_u8]));
    }
    for object in [0x910, 0x911, 0x912] {
        let mut geometry = geometry();
        let cell = if object == 0x912 { 0x901 } else { 0x900 };
        geometry.parent = Parent::Cell(cell);
        let mut record = Record::new(Tag(*b"NAVM"), object).sub(Tag(*b"NVNM"), geometry.to_bytes());
        record.placement.cell = Some(cell);
        plugin.push_top(record);
    }
    for object in [7, 0x800] {
        plugin.push_top(Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"ACBS"), vec![0; 24]));
    }
    for object in [0x14, 0x801] {
        let mut record = Record::new(Tag(*b"ACHR"), object)
            .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]);
        record.placement.cell = Some(0x900);
        plugin.push_top(record);
    }
    plugin.push_top(Record::new(Tag(*b"DOOR"), 0x820));
    for object in [0x830, 0x831] {
        let mut record = Record::new(Tag(*b"REFR"), object)
            .sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]);
        record.placement.cell = Some(if object == 0x830 { 0x900 } else { 0x901 });
        if object == 0x830 {
            record = record.sub(
                Tag(*b"XTEL"),
                [0x831_u32.to_le_bytes().as_slice(), &[0_u8; 24]].concat(),
            );
        }
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Draw.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().load_cell(id(0x900));
    game.world_mut().actor(id(0x801)).unwrap().ai.path = vec![[10.0, 10.0, 0.0], [20.0, 20.0, 0.0]];
    game.world_mut().reference(id(0x14)).unwrap().position = [10.0, 10.0, 0.0];
    game
}

#[test]
fn tnm_draws_only_loaded_triangles_with_links_cover_and_player_highlight() {
    let mut game = game();
    Console::new().run(&mut game, "tnm").unwrap();
    let mut world = game.world_mut();
    let meshes = loaded_navmeshes(&world);
    let frame = frame(&mut world, &game.debug_views, &meshes, id(0x14));
    assert_eq!(frame.navmesh_triangles, 4);
    assert_eq!(
        frame.triangles.len(),
        meshes
            .iter()
            .map(|mesh| mesh.geometry.triangles.len())
            .sum::<usize>()
    );
    assert_eq!(
        (
            frame.edge_links,
            frame.door_links,
            frame.teleport_links,
            frame.cover_edges,
            frame.highlighted
        ),
        (2, 2, 2, 4, 1)
    );
}

#[test]
fn tpl_geometry_counts_each_live_waypoint_and_segment() {
    let mut game = game();
    Console::new().run(&mut game, "tpl").unwrap();
    let mut world = game.world_mut();
    let expected = world.actor_ref(id(0x801)).unwrap().ai.path.len();
    let frame = frame(&mut world, &game.debug_views, &[], id(0x14));
    assert_eq!(
        (frame.path_segments, frame.waypoints, frame.lines.len()),
        (expected, expected, expected * 4)
    );
}

#[test]
fn showpivot_draws_three_axes_and_disabling_draws_nothing() {
    let mut game = game();
    let mut console = Console::new();
    console.select(id(0x801));
    console.run(&mut game, "sp").unwrap();
    assert_eq!(
        frame(&mut game.world_mut(), &game.debug_views, &[], id(0x14))
            .lines
            .len(),
        3
    );
    console.run(&mut game, "sp").unwrap();
    let frame = frame(&mut game.world_mut(), &game.debug_views, &[], id(0x14));
    assert!(frame.lines.is_empty() && frame.triangles.is_empty() && frame.text.is_empty());
}

#[test]
fn tdt_labels_the_live_procedure_and_path_state() {
    let mut game = game();
    Console::new().run(&mut game, "tdt").unwrap();
    let frame = frame(&mut game.world_mut(), &game.debug_views, &[], id(0x14));
    assert!(frame.text.join("\n").contains("waypoints=2"));
}

#[test]
fn navmesh_palette_uses_cover_enum_for_ledge_and_separate_flag_colours() {
    let colors = [
        triangle_color(0x200, 0, false, false),
        triangle_color(0x400, 0, false, false),
        triangle_color(0x40, 0, false, false),
        triangle_color(0, 2, false, false),
    ];
    for (index, color) in colors.iter().enumerate() {
        assert!(!colors[..index].contains(color));
    }
}

#[test]
fn ssg_reports_accumulated_transforms_controllers_collision_and_cycles() {
    use soe_nif::blocks::{AvObject, Block, Node, ObjectNet};
    use soe_nif::{BlockRef, Transform, Vec3};
    let mut root = Node {
        av: AvObject {
            net: ObjectNet {
                name: "root".into(),
                controller: BlockRef(2),
                ..ObjectNet::default()
            },
            collision: BlockRef(3),
            ..AvObject::default()
        },
        children: vec![BlockRef(1)],
        ..Node::default()
    };
    root.av.transform.translation = Vec3::new(10.0, 0.0, 0.0);
    let mut child = root.clone();
    child.av.net.name = "child".into();
    child.av.transform.translation = Vec3::new(5.0, 0.0, 0.0);
    child.children = vec![BlockRef(0)];
    let file = soe_nif::NifFile {
        header: soe_nif::Header {
            version: soe_nif::header::VERSION_20_2_0_7,
            user_version: 12,
            bs_version: 100,
            author: String::new(),
            process_script: String::new(),
            export_script: String::new(),
            max_filepath: String::new(),
            block_types: Vec::new(),
            block_type_index: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            groups: Vec::new(),
        },
        roots: vec![BlockRef(0)],
        blocks: vec![
            Block::Node(root),
            Block::Node(child),
            Block::TransformController(soe_nif::controller::TransformController {
                time: soe_nif::controller::TimeController {
                    next_controller: BlockRef::NONE,
                    ..Default::default()
                },
                interpolator: BlockRef::NONE,
            }),
            Block::Collision {
                type_name: "bhkCollisionObject".into(),
                data: Vec::new(),
            },
        ],
    };
    let report = scenegraph::report(&file).join("\n");
    assert!(
        report.contains("root")
            && report.contains("child")
            && report.contains("NiTransformController")
            && report.contains("bhkCollisionObject")
            && report.contains("cyclic")
    );
    assert!(report.contains(&format!(
        "accumulated={:?}",
        Transform {
            translation: Vec3::new(15.0, 0.0, 0.0),
            ..Transform::default()
        }
    )));
}

#[test]
fn debug_views_installed_loaded_navmesh_counts_and_player_path_query() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut order = LoadOrder::new();
    for master in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let plugin = soe_esm::read_plugin(std::path::Path::new(&directory).join(master))
            .unwrap()
            .unwrap();
        order.add(master, plugin).unwrap();
    }
    let mut game = Game::new(order);
    let cell = game.form("WhiterunBanneredMare").unwrap();
    game.world_mut().load_cell(cell);
    let player = game.player();
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).unwrap();
        reference.cell = Some(cell);
        reference.position = [-125.0, -800.0, 120.0];
    }
    Console::new().run(&mut game, "tnm").unwrap();
    let mut world = game.world_mut();
    let meshes = loaded_navmeshes(&world);
    let expected: usize = meshes
        .iter()
        .map(|mesh| mesh.geometry.triangles.len())
        .sum();
    assert!(
        expected > 100,
        "installed inn should have over 100 authored triangles"
    );
    let frame = frame(&mut world, &game.debug_views, &meshes, player);
    assert_eq!(frame.navmesh_triangles, expected);
    assert_eq!(frame.highlighted, 1);
    drop(world);
    let report = Console::new()
        .run(&mut game, "player.p2p -125 -800 120")
        .unwrap()
        .to_string();
    assert!(!report.contains("failed:"), "{report}");
}

#[cfg(feature = "godot")]
#[test]
fn debug_views_headless_walkthrough_overlay() {
    let (Some(_data), Some(binary)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    let user = workspace.join("target/debugviews-user");
    let import = std::process::Command::new(&binary)
        .env("DISPLAY", ":99")
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_DATA_HOME", &user)
        .args(["--headless", "--path"])
        .arg(&project)
        .args(["--editor", "--import", "--quit"])
        .output()
        .unwrap();
    assert!(
        import.status.success(),
        "Godot import: {}",
        String::from_utf8_lossy(&import.stderr)
    );
    let output = std::process::Command::new(&binary)
        .env("DISPLAY", ":99")
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_DATA_HOME", user)
        .args(["--headless", "--path"])
        .arg(&project)
        .args(["--script", "res://debug_views_check.gd"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        workspace.join("target/debug-views-host.log"),
        format!("{stdout}\n{stderr}\nexit={}\n", output.status),
    )
    .unwrap();
    assert!(
        output.status.success() && stdout.contains("DEBUG_VIEWS passed"),
        "headless walkthrough exit={}\n{stdout}\n{stderr}",
        output.status
    );
}
