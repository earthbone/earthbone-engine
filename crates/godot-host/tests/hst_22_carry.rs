// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-22 NIF motion types and installed Bannered Mare carry acceptance.
//! Uses nif.xml hkMotionType through the existing bhk decoder.

use std::path::Path;
use std::process::Command;

use soe_esm::records::refr::Refr;
use soe_godot_host::data_source::DataSource;
use soe_physics::{Collider, Iso, Layer, MotionType, Shape, Vec3};

#[test]
fn hst_22_dynamic_clutter_moves_while_fixed_keyframed_and_zero_mass_stay_static() {
    let mut collider = Collider {
        node: soe_nif::BlockRef::NONE,
        node_name: "clutter".into(),
        layer: Layer::CLUTTER,
        motion: MotionType::Dynamic,
        mass: 1.0,
        transform: Iso::IDENTITY,
        shape: Shape::Box {
            half_extents: Vec3::new(1.0, 1.0, 1.0),
        },
    };
    assert!(!collider.is_static());
    for motion in [MotionType::Fixed, MotionType::Keyframed] {
        collider.motion = motion;
        assert!(collider.is_static(), "{motion:?}");
    }
    collider.motion = MotionType::Dynamic;
    collider.mass = 0.0;
    assert!(collider.is_static());
}

#[test]
fn hst_22_bannered_mare_tankard_carries_two_metres_collides_and_lands() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let order = source.shared_order();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let (placed, _) = source.placed_cell_colliders(cell).unwrap();
    let tankard = placed
        .iter()
        .find(|placed| {
            let winner = order.winner_ref(placed.reference).unwrap();
            let reference = Refr::from_record(order.record(winner)).unwrap();
            let base = order.resolve(winner.file, reference.base);
            order
                .winner(base)
                .and_then(|record| record.editor_id())
                .is_some_and(|name| name.to_ascii_lowercase().contains("tankard"))
                && !placed.collider.is_static()
        })
        .expect("authored Bannered Mare tankard with dynamic bhk collision");
    assert_eq!(tankard.collider.motion, MotionType::Dynamic);
    eprintln!(
        "HST-22 tankard {} at {:?}",
        tankard.reference, tankard.original.translation
    );
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(godot)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_22_carry_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_HST_22_FILE", tankard.reference.file.to_string())
        .env("SOE_HST_22_OBJECT", tankard.reference.object.to_string())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        root.join("target/hst-22-scene.log"),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    let report = stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| line.contains("HST-22") || line.contains("SCRIPT ERROR"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && stdout.contains("HST-22 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{report}\nFull output: target/hst-22-scene.log"
    );
    eprintln!("{report}");
}
