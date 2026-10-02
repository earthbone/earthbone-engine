// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::image_space::WeatherSelection;
use soe_game::World;
use soe_godot_host::image_space::{ImageSpaceStack, RenderedBase};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn fixture_values() -> String {
    let mut plugin = Plugin::new(FileHeader::default());
    for (form, white, bloom, saturation, brightness, contrast, tint, depth) in [
        (
            10,
            4.0,
            0.4,
            0.6,
            1.2,
            1.4,
            [0.25, 0.8, 0.2, 0.1],
            [0.5, 400.0, 200.0],
        ),
        (
            11,
            8.0,
            0.8,
            0.4,
            1.6,
            1.1,
            [0.5, 0.1, 0.8, 0.2],
            [0.8, 600.0, 300.0],
        ),
    ] {
        plugin.push_top(
            Record::new(Tag::new(b"IMGS"), form)
                .sub(
                    Tag::new(b"HNAM"),
                    floats(&[0.0, 0.0, 0.0, bloom, 0.0, white, 1.0, 1.0, 0.5]),
                )
                .sub(
                    Tag::new(b"CNAM"),
                    floats(&[saturation, brightness, contrast]),
                )
                .sub(Tag::new(b"TNAM"), floats(&tint))
                .sub(
                    Tag::new(b"DNAM"),
                    [floats(&depth), vec![1, 0, 0, 64]].concat(),
                ),
        );
    }
    plugin
        .push_top(Record::new(Tag::new(b"CELL"), 20).sub(Tag::new(b"XCIM"), 10_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag::new(b"WTHR"), 30).sub(
            Tag::new(b"IMSP"),
            [10_u32, 11, 10, 11]
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("Mag72Render.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.load_cell(id(20));
    let mut stack = ImageSpaceStack::default();
    stack.sync(&world).unwrap();
    let cell = RenderedBase::from(stack.base().unwrap());
    world
        .select_weather_image_space(Some(WeatherSelection {
            weather: id(30),
            from: 0,
            to: 1,
            blend: 0.5,
        }))
        .unwrap();
    stack.sync(&world).unwrap();
    let weather = RenderedBase::from(stack.base().unwrap());
    world.select_weather_image_space(None).unwrap();
    stack.sync(&world).unwrap();
    let restored = RenderedBase::from(stack.base().unwrap());
    let encode = |base: &RenderedBase| {
        format!(
            "{{\"exposure\":{},\"white\":{},\"bloom\":{},\"saturation\":{},\"brightness\":{},\"contrast\":{},\"tint\":[{},{},{},{}],\"depth_amount\":{},\"depth_distance\":{},\"depth_range\":{}}}",
            base.exposure,
            base.white,
            base.bloom,
            base.saturation,
            base.brightness,
            base.contrast,
            base.tint[0],
            base.tint[1],
            base.tint[2],
            base.tint[3],
            base.depth_amount,
            base.depth_distance,
            base.depth_range
        )
    };
    format!(
        "{{\"cell\":{},\"weather\":{},\"restored\":{}}}",
        encode(&cell),
        encode(&weather),
        encode(&restored)
    )
}

#[test]
fn mag_72_windowed_fixture_reads_hdr_color_and_depth_pixels() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN is unset, skipping MAG-72 rendered pixel check");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = manifest.join("../../godot");
    let script = project.join("mag_72_image_space_render_test.gd");
    let output = Command::new(godot)
        .args(["--path"])
        .arg(&project)
        .args(["--script"])
        .arg(script)
        .args(["--resolution", "640x360"])
        .env("MAG_72_RENDER_CASES", fixture_values())
        .output()
        .expect("Godot starts for the MAG-72 windowed render fixture");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "MAG-72 Godot render fixture failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
