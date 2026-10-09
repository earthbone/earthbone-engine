// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

//! Adversarial WTHR cloud speed decoding cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::sky::Climate, World};
use std::{path::PathBuf, process::Command};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn climate() -> Climate {
    Climate {
        timing: [6.0, 8.0, 18.0, 20.0],
        moons: [false; 2],
        phase_days: 0,
        sun_texture: String::new(),
        glare_texture: String::new(),
        night_model: String::new(),
        weathers: Vec::new(),
    }
}

fn weather(x: Option<Vec<u8>>, y: Option<Vec<u8>>) -> Record {
    Record::new(Tag(*b"WTHR"), 10)
        .sub(b"NAM0", vec![0; 17 * 16])
        .sub(b"DATA", vec![0; 19])
        .sub(b"FNAM", vec![0; 32])
        .sub(b"LNAM", 1_u32.to_le_bytes())
        .sub(b"00TX", b"sky/cloud.dds\0".to_vec())
        .with_optional_subrecord(*b"QNAM", x)
        .with_optional_subrecord(*b"RNAM", y)
}

trait OptionalSubrecord {
    fn with_optional_subrecord(self, tag: [u8; 4], value: Option<Vec<u8>>) -> Self;
}

impl OptionalSubrecord for Record {
    fn with_optional_subrecord(mut self, tag: [u8; 4], value: Option<Vec<u8>>) -> Self {
        if let Some(value) = value {
            self = self.sub(tag, value);
        }
        self
    }
}

fn sample(
    record: Record,
) -> Result<soe_game::world::sky::SkyFrame, impl std::fmt::Debug + std::fmt::Display> {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order
        .add("Clouds.esm", plugin)
        .expect("add weather fixture");
    World::new(order).sample_sky(id(10), &climate(), 12.0, 0.0)
}

#[test]
fn rnd_25_wthr_speed_endpoints_and_zero_decode_exactly() {
    let mut x = vec![127; 32];
    x[0] = 0;
    x[1] = 254;
    let mut y = vec![127; 32];
    y[0] = 254;
    y[1] = 0;
    let frame = sample(weather(Some(x), Some(y))).expect("valid WTHR speeds");
    assert_eq!(frame.clouds.len(), 1);
    assert_eq!(frame.clouds[0].speed, [-0.1, 0.1]);
}

#[test]
fn rnd_25_missing_both_speed_axes_mean_stationary_clouds() {
    let frame = sample(weather(None, None)).expect("WTHR without optional axes");
    assert_eq!(frame.clouds.len(), 1);
    assert_eq!(frame.clouds[0].speed, [0.0, 0.0]);
}

#[test]
fn rnd_25_short_x_speed_array_is_rejected() {
    let error = sample(weather(Some(vec![0; 31]), None)).expect_err("short QNAM is malformed");
    assert!(error.to_string().contains("invalid cloud speed size"));
}

#[test]
fn rnd_25_short_y_speed_array_is_rejected() {
    let error = sample(weather(None, Some(vec![254; 31]))).expect_err("short RNAM is malformed");
    assert!(error.to_string().contains("invalid cloud speed size"));
}

#[test]
fn rnd_25_installed_skyrimcloudy_scroll_rate_matches_its_wthr_speeds() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let output = Command::new(godot)
        .arg("--path")
        .arg(project)
        .args([
            "--resolution",
            "640x360",
            "--script",
            "rnd_25_cloud_speed_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run installed SkyrimCloudy speed check");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success()
            && stdout.contains("RND-25 RESULT: PASS")
            && !stderr.contains("SCRIPT ERROR:")
            && !stderr.contains("SHADER ERROR:"),
        "RND-25 installed cloud rate failed\n{stdout}\n{stderr}"
    );
}
