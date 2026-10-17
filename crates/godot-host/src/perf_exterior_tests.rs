// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed PRF-40 traversals use the ordinary walkthrough and stock Godot.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[test]
fn prf_42_static_cell_updates_do_not_rebuild_the_resident_world_grid() {
    use soe_physics::{Collider, Iso, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3};
    use std::time::{Duration, Instant};

    const LIMIT: Duration = Duration::from_millis(33);
    fn terrain(repetitions: usize) -> Collider {
        let vertices = (0..33)
            .flat_map(|y| (0..33).map(move |x| Vec3::new(x as f32 * 128.0, y as f32 * 128.0, 0.0)))
            .collect();
        let mut triangles = Vec::new();
        for _ in 0..repetitions {
            for y in 0..32u32 {
                for x in 0..32u32 {
                    let sw = y * 33 + x;
                    triangles.extend([[sw, sw + 1, sw + 34], [sw, sw + 34, sw + 33]]);
                }
            }
        }
        Collider {
            node: soe_nif::BlockRef(0),
            node_name: "streamed terrain".into(),
            layer: Layer::STATIC,
            motion: MotionType::Fixed,
            mass: 0.0,
            transform: Iso::IDENTITY,
            shape: Shape::TriangleMesh {
                radius: 0.0,
                vertices,
                triangles,
                materials: Vec::new(),
            },
        }
    }
    let mut world = SimpleWorld::new();
    let mut resident = terrain(10);
    for y in 0..5 {
        for x in 0..5 {
            resident.transform.translation = Vec3::new(x as f32 * 4096.0, y as f32 * 4096.0, 0.0);
            world.add_static(&resident);
        }
    }
    assert_eq!(world.primitive_count(), 512_000);
    world.step(0.0);
    let mut arriving = terrain(1);
    arriving.transform.translation = Vec3::new(-4096.0, 0.0, 0.0);
    let from = Vec3::new(-2048.0, 2048.0, 100.0);
    let to = Vec3::new(-2048.0, 2048.0, -100.0);
    let start = Instant::now();
    let body = world.add_static(&arriving);
    world.step(0.0);
    let added = start.elapsed();
    assert_eq!(world.raycast(from, to).unwrap().body, body);
    let start = Instant::now();
    world.remove(body);
    world.step(0.0);
    let removed = start.elapsed();
    assert!(world.raycast(from, to).is_none());
    assert!(world
        .raycast(
            Vec3::new(2048.0, 2048.0, 100.0),
            Vec3::new(2048.0, 2048.0, -100.0)
        )
        .is_some());
    eprintln!(
        "PRF42_STATIC_UPDATE primitives=512000 added_ms={:.3} removed_ms={:.3} limit_ms=33",
        added.as_secs_f64() * 1000.0,
        removed.as_secs_f64() * 1000.0
    );
    assert!(
        added < LIMIT && removed < LIMIT,
        "static-cell mutation revisited the resident world: add={added:?}, remove={removed:?}"
    );
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[test]
fn installed_tree_pipeline_material_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open_with_threads(Path::new(&data), 1).unwrap();
    for model in [
        "landscape/trees/treepineforest01.nif",
        "landscape/trees/treeaspen01.nif",
    ] {
        let surfaces = source.item_preview_surfaces(model).unwrap();
        assert!(!surfaces.is_empty());
        for (arrays, _) in surfaces {
            eprintln!(
                "R4_TREE_MATERIAL model={model} vertices={} material={:?}",
                arrays.positions.len(),
                arrays.material
            );
        }
    }
}

#[test]
fn r4_installed_game_time_tracks_wall_time_through_the_inn_exit() {
    r4_installed_route("r4_runtime_test.gd");
}

#[test]
fn r4_installed_full_tree_pipelines_are_warm() {
    r4_installed_route("r4_tree_pipeline_test.gd");
}

fn r4_installed_route(script: &str) {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = root();
    let directory = root
        .join("target/r4-runtime")
        .join(script.trim_end_matches(".gd"));
    std::fs::create_dir_all(&directory).unwrap();
    let report = directory.join("clock.json");
    let output = std::fs::File::create(directory.join("clock.log")).unwrap();
    let errors = std::fs::File::create(directory.join("clock.err")).unwrap();
    let status = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args(["--resolution", "640x360", "--script", script])
        .env("SKYRIM_DATA", data)
        .env("SOE_R4_REPORT", &report)
        .env("SOE_GODOT_TIMEOUT", "600")
        .env("DISPLAY", ":99")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("SOE_THREADED_SIMULATION")
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors)
        .status()
        .unwrap();
    let log = std::fs::read_to_string(directory.join("clock.log")).unwrap();
    let errors = std::fs::read_to_string(directory.join("clock.err")).unwrap();
    assert!(status.success(), "{status}: {log}\n{errors}");
    assert!(!errors.contains("SCRIPT ERROR:"), "{errors}");
    assert!(log.contains("R4_RUNTIME PASS"), "{log}");
    assert!(log.contains("WALKTHROUGH stopped: ready=true"), "{log}");
    let row: serde_json::Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    for phase in ["inn_20fps", "door_walk_60fps"] {
        let ratio = row[phase]["ratio"].as_f64().unwrap();
        assert!((ratio - 1.0).abs() < 0.05, "{phase}: {ratio}");
        assert!(row[phase]["distance"].as_f64().unwrap() > 100.0);
        println!(
            "R4_CLOCK {phase} ratio={ratio} wall={} game={}",
            row[phase]["wall"], row[phase]["game"]
        );
    }
    let trace: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report.with_extension("trace.json")).unwrap())
            .unwrap();
    assert!(trace["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["ph"] == "X"));
}

#[test]
fn prf_40_installed_exterior_uploads_and_physics_are_bounded() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let root = root();
    let directory = root.join("target/prf40-after");
    std::fs::create_dir_all(&directory).unwrap();
    for cell in ["WhiterunExterior04", "WhiterunExterior01"] {
        let report = directory.join(format!("{cell}.json"));
        let stdout = directory.join(format!("{cell}.log"));
        let stderr = directory.join(format!("{cell}.err"));
        let output = std::fs::File::create(&stdout).unwrap();
        let errors = std::fs::File::create(&stderr).unwrap();
        let status = Command::new(&godot)
            .args(["--path"])
            .arg(root.join("godot"))
            .args([
                "--resolution",
                "640x360",
                "--script",
                "prf_40_exterior_test.gd",
            ])
            .env("SKYRIM_DATA", &data)
            .env("SOE_PRF40_CELL", cell)
            .env("SOE_PRF40_REPORT", &report)
            .env("XDG_DATA_HOME", directory.join(format!("{cell}-userdata")))
            .env("SOE_GODOT_TIMEOUT", "900")
            .env("DISPLAY", ":99")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("SOE_THREADED_SIMULATION")
            .env_remove("SOE_CAPTURE_PATH")
            .env_remove("SOE_AUD_11_TEST")
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(errors)
            .status()
            .unwrap();
        let log = std::fs::read_to_string(&stdout).unwrap();
        let errors = std::fs::read_to_string(&stderr).unwrap();
        assert!(
            status.success(),
            "{cell}: {status}, see {} and {}",
            stdout.display(),
            stderr.display()
        );
        assert!(
            !errors.contains("SCRIPT ERROR:"),
            "{cell}: script failed, see {}",
            stderr.display()
        );
        assert!(log.contains("prf_40_installed_exterior: PASS"));
        assert!(log.contains("WALKTHROUGH stopped: ready=true"));
        let trace: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report.with_extension("trace.json")).unwrap())
                .unwrap();
        assert!(
            trace["traceEvents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["ph"] == "X"),
            "{cell}: load trace contains no timed events"
        );
        let row: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        assert_eq!(row["frames"].as_array().unwrap().len(), 360);
        assert!(
            row["max_physics_steps"].as_u64().unwrap() <= 1,
            "{cell}: {}",
            row["max_physics_steps"]
        );
        assert!(
            row["upload"]["slices"].as_u64().unwrap() > 1,
            "{cell}: publication did not yield"
        );
        assert!(
            row["upload"]["worst_ms"].as_f64().unwrap() < 33.0,
            "{cell}: upload breached 33 ms: {}",
            row["upload"]
        );
        assert!(
            row["physics_ms_max"].as_f64().unwrap() < 33.0,
            "{cell}: physics callback breached 33 ms: {}",
            row["physics_ms_max"]
        );
        assert_eq!(
            row["pipelines"]["draw"].as_u64().unwrap(),
            0,
            "{cell}: foreground draw compilation"
        );
        assert!(
            row["horizontal_distance"].as_f64().unwrap() > 128.0,
            "{cell}: no exterior traversal"
        );
        assert_ne!(
            row["start_cell"], row["end_cell"],
            "{cell}: no boundary crossing"
        );
        println!("PRF40 {cell}: streaming p50={} p99={} max={}, upload={}, physics_steps={}, pipelines={}", row["streaming_ms_p50"], row["streaming_ms_p99"], row["streaming_ms_max"], row["upload"], row["max_physics_steps"], row["pipelines"]);
    }
}
