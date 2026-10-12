// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedModifiers};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn modifier(object: u32, channels: &[([u8; 4], usize, bool, Vec<f32>)]) -> Record {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1u32.to_le_bytes());
    header[4..8].copy_from_slice(&2.0f32.to_le_bytes());
    let mut record = Record::new(Tag::new(b"IMAD"), object);
    for (tag, count_offset, color, values) in channels {
        header[*count_offset..*count_offset + 4].copy_from_slice(&2_u32.to_le_bytes());
        let mut data = Vec::new();
        for time in [0.0_f32, 1.0] {
            data.extend_from_slice(&time.to_le_bytes());
            for value in values {
                data.extend_from_slice(&(value * time * 2.0).to_le_bytes());
            }
        }
        assert_eq!(data.len(), if *color { 40 } else { 16 });
        record = record.sub(Tag::new(tag), data);
    }
    record.sub(Tag::new(b"DNAM"), header)
}

fn composed_fixture() -> String {
    let channels = [
        (*b"TNAM", 176, true, vec![1.0, 0.05, 0.02, 0.8]),
        (*b"BNAM", 180, false, vec![6.0]),
        (*b"WNAM", 212, false, vec![0.8]),
        (*b"XNAM", 216, false, vec![1000.0]),
        (*b"YNAM", 220, false, vec![500.0]),
        (*b"\x02IAD", 8 + 2 * 8, false, vec![0.1]),
        (*b"\x03IAD", 8 + 3 * 8, false, vec![3.0]),
        (*b"\x11IAD", 8 + 17 * 8, false, vec![0.25]),
        (*b"\x12IAD", 8 + 18 * 8, false, vec![1.4]),
        (*b"\x13IAD", 8 + 19 * 8, false, vec![1.5]),
    ];
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(modifier(1, &channels));
    let mut order = LoadOrder::new();
    order.add("Mag71Rendered.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().apply_image_space(id(1), 0.5).unwrap();
    game.tick(1.0);
    let mut stack = ImageSpaceStack::default();
    stack.sync(&game.world()).unwrap();
    let layers = stack.render_layers(&game.world()).unwrap();
    let composed = RenderedModifiers::from_layers(&layers);

    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].strength, 0.5);
    assert_eq!(layers[0].channels.len(), channels.len());
    assert!((composed.tint[0] - 0.5).abs() < 0.00001);
    assert!((composed.tint[3] - 0.4).abs() < 0.00001);
    assert!((composed.blur_radius - 3.0).abs() < 0.00001);
    assert!((composed.depth_strength - 0.4).abs() < 0.00001);
    assert!((composed.depth_distance - 1000.0 / 140.0).abs() < 0.00001);
    assert!((composed.depth_range - (1.0 + 500.0 / 140.0)).abs() < 0.00001);
    assert!((composed.bloom_threshold - 0.4).abs() < 0.00001);
    assert!((composed.bloom_scale - 1.5).abs() < 0.00001);
    assert!((composed.saturation - 0.625).abs() < 0.00001);
    assert!((composed.brightness - 1.2).abs() < 0.00001);
    assert!((composed.contrast - 1.25).abs() < 0.00001);
    format!(
        "{{\"tint\":[{},{},{},{}],\"blur_radius\":{},\"double_vision\":{},\"depth_strength\":{},\"depth_distance\":{},\"depth_range\":{},\"bloom_threshold\":{},\"bloom_scale\":{},\"saturation\":{},\"brightness\":{},\"contrast\":{}}}",
        composed.tint[0],
        composed.tint[1],
        composed.tint[2],
        composed.tint[3],
        composed.blur_radius,
        composed.double_vision,
        composed.depth_strength,
        composed.depth_distance,
        composed.depth_range,
        composed.bloom_threshold,
        composed.bloom_scale,
        composed.saturation,
        composed.brightness,
        composed.contrast
    )
}

#[test]
fn mag_71_rendered_authored_imad_stack_changes_pixels_at_animated_weights() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN is unset, skipping MAG-71 rendered IMAD check");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("mag_71_rendered_image_space_test.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--script"])
        .arg(script)
        .env("MAG_71_RENDER_CASE", composed_fixture())
        .output()
        .expect("Godot starts for the MAG-71 rendered IMAD fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "MAG-71 rendered IMAD fixture failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
