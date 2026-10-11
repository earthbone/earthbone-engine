// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-23 windowed NPC pixels and TST-18 headless frame budgets on installed data.

use soe_esm::{GlobalFormId, Tag};
use soe_godot_host::data_source::DataSource;
use std::path::{Path, PathBuf};

fn target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"))
}

#[derive(Clone, Copy)]
enum HostMode {
    Windowed,
    Headless,
    Threaded,
}

fn run(script: &str, mode: HostMode) -> (bool, String) {
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("host check requires SOE_GODOT_BIN");
    let mut command = std::process::Command::new(binary);
    command
        .arg("--path")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot"));
    if matches!(mode, HostMode::Headless) {
        command.arg("--headless");
    } else {
        command.args(["--resolution", "640x360"]);
    }
    command.args(["--script", script]);
    command
        .env(
            "SOE_THREADED_SIMULATION",
            if matches!(mode, HostMode::Threaded) {
                "1"
            } else {
                "0"
            },
        )
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST");
    let output = command.output().unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(target().join(format!("{script}.log")), &log).unwrap();
    let summary = log
        .lines()
        .filter(|line| {
            line.starts_with("HST-23")
                || line.starts_with("TST-18")
                || line.starts_with("RESULT:")
                || line.starts_with("FAIL ")
                || line.starts_with("SCRIPT ERROR:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("{script}: {summary}");
    println!(
        "{script}: exit {:?}, full log in target/{script}.log",
        output.status.code()
    );
    (
        output.status.success() && !log.contains("SCRIPT ERROR:"),
        log,
    )
}

fn identity(id: GlobalFormId) -> serde_json::Value {
    serde_json::json!({"file": id.file, "object": id.object})
}

#[test]
fn hst_23_installed_npc_pixels_residency_movement_and_tst_18_budget() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let cell = game.form("WhiterunBanneredMare").unwrap();
    game.change_cell(Some(cell));
    let contacts = game.world_mut().contact_actors();
    let hulda = contacts
        .iter()
        .find(|actor| game.world().name_of(actor.actor).as_deref() == Some("Hulda"))
        .expect("Hulda is placed in the inn")
        .actor;
    let order = source.shared_order();
    let exit = source
        .cell_index()
        .references(cell)
        .iter()
        .copied()
        .find(|id| {
            let mut world = game.world_mut();
            world
                .reference(*id)
                .is_some_and(|reference| reference.teleport.is_some())
        })
        .expect("Bannered Mare exit door");
    let destination = game
        .world_mut()
        .reference(exit)
        .unwrap()
        .teleport
        .unwrap()
        .0;
    let (parent, door_position) = {
        let mut world = game.world_mut();
        let reference = world.reference(destination).unwrap();
        (reference.cell.unwrap(), reference.position)
    };
    let worldspace = game.world().cell_coordinate_space(parent);
    let exterior_cell = source
        .cell_index()
        .exterior_cell(
            worldspace,
            (door_position[0] / 4096.0).floor() as i32,
            (door_position[1] / 4096.0).floor() as i32,
        )
        .expect("grid outside Bannered Mare");
    let exterior_name = parent.to_string();
    game.change_cell(Some(exterior_cell));
    let grid = source.cell_index().grid_of(exterior_cell).unwrap();
    let mut neighbors: Vec<_> = (-2..=2)
        .flat_map(|x| (-2..=2).map(move |y| (x, y)))
        .filter_map(|(x, y)| {
            source
                .cell_index()
                .exterior_cell(grid.world, grid.x + x, grid.y + y)
        })
        .filter(|id| *id != exterior_cell)
        .collect();
    neighbors.extend(source.cell_index().world_persistent_cell(grid.world));
    game.attach_streaming_cells(&neighbors).unwrap();
    let contacts = game.world_mut().contact_actors();
    let mut guards: Vec<_> = contacts
        .into_iter()
        .filter(|actor| {
            game.world()
                .name_of(actor.actor)
                .is_some_and(|name| name.contains("Guard"))
        })
        .collect();
    guards.sort_by(|a, b| {
        let distance = |position: [f32; 3]| {
            position
                .into_iter()
                .zip(door_position)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>()
        };
        distance(a.position).total_cmp(&distance(b.position))
    });
    let guard = guards.first().expect("Whiterun guard").actor;
    for actor in [hulda, guard] {
        assert_eq!(order.winner(actor).unwrap().tag, Tag(*b"ACHR"));
        let appearance = game.world_mut().actor_appearance(actor).unwrap();
        assert!(!appearance.parts.is_empty() && !appearance.head_parts.is_empty());
    }
    let manifest = serde_json::json!({"hulda": identity(hulda), "guard": identity(guard), "exit": identity(exit),
        "exterior_cell": exterior_name});
    std::fs::create_dir_all(target()).unwrap();
    std::fs::write(
        target().join("hst_23_manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    drop(game);
    drop(source);
    drop(order);
    let checks = [
        (
            "hst_23_characters_test.gd",
            HostMode::Windowed,
            "RESULT: PASS hst_23_characters",
        ),
        (
            "hst_23_snapshot_test.gd",
            HostMode::Threaded,
            "RESULT: PASS hst_23_snapshot",
        ),
        (
            "hst_23_frame_budget_test.gd",
            HostMode::Headless,
            "RESULT: PASS",
        ),
    ];
    let results: Vec<_> = checks
        .into_iter()
        .map(|(script, mode, marker)| {
            let (success, log) = run(script, mode);
            (script, success && log.contains(marker))
        })
        .collect();
    assert!(results.iter().all(|(_, passed)| *passed), "{results:?}");
}
