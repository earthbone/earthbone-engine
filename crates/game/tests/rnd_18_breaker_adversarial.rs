// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(local: Option<Vec<u8>>, template: Option<Record>, link: Option<u32>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    if let Some(template) = template {
        plugin.push_top(template);
    }
    let mut cell = Record::new(Tag(*b"CELL"), 31).sub(b"DATA", [1]);
    if let Some(local) = local {
        cell = cell.sub(b"XCLL", local);
    }
    if let Some(link) = link {
        cell = cell.sub(b"LTMP", link.to_le_bytes());
    }
    plugin.push_top(cell);
    let mut order = LoadOrder::new();
    order.add("Lighting.esm", plugin).expect("valid fixture");
    World::new(order)
}

fn lighting(color: [u8; 3], fog_clip: f32, fade_begin: f32, fade_end: f32) -> Vec<u8> {
    let mut bytes = vec![0; 92];
    bytes[..3].copy_from_slice(&color);
    bytes[12..16].copy_from_slice(&100.0_f32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1000.0_f32.to_le_bytes());
    bytes[32..36].copy_from_slice(&fog_clip.to_le_bytes());
    bytes[36..40].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[80..84].copy_from_slice(&fade_begin.to_le_bytes());
    bytes[84..88].copy_from_slice(&fade_end.to_le_bytes());
    bytes
}

fn template(data: Vec<u8>, dalc: Option<Vec<u8>>) -> Record {
    let mut record = Record::new(Tag(*b"LGTM"), 40).sub(b"DATA", data);
    if let Some(dalc) = dalc {
        record = record.sub(b"DALC", dalc);
    }
    record
}

#[test]
fn rnd_18_breaker_rejects_unresolved_template_when_local_fields_inherit() {
    let mut local = lighting([10, 20, 30], 10.0, 20.0, 30.0);
    local[88..92].copy_from_slice(&1_u32.to_le_bytes());
    let result = world(Some(local), None, Some(99)).interior_lighting(id(31));
    assert!(result.is_err(), "inherited ambient has no resolvable LGTM");
}

#[test]
fn rnd_18_breaker_rejects_truncated_directional_ambient_colors() {
    let result = world(
        None,
        Some(template(
            lighting([1, 2, 3], 800.0, 100.0, 500.0),
            Some(vec![1; 25]),
        )),
        Some(40),
    )
    .interior_lighting(id(31));
    assert!(result.is_err(), "DALC must contain complete color fields");
}

#[test]
fn rnd_18_breaker_inherits_fog_clip_and_light_fade_independently() {
    let mut local = lighting([10, 20, 30], 10.0, 20.0, 30.0);
    local[88..92].copy_from_slice(&0x480_u32.to_le_bytes());
    let result = world(
        Some(local),
        Some(template(
            lighting([60, 90, 120], 800.0, 200.0, 900.0),
            Some(vec![100; 24]),
        )),
        Some(40),
    )
    .interior_lighting(id(31))
    .expect("valid inherited lighting");
    assert_eq!(result.fog_clip, 800.0);
    assert_eq!(result.light_fade, [200.0, 900.0]);
    assert_eq!(result.ambient, [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]);
}

#[test]
fn rnd_18_breaker_rejects_nonfinite_optional_template_light_fade() {
    let result = world(
        None,
        Some(template(
            lighting([1, 2, 3], 800.0, f32::NAN, 500.0),
            Some(vec![0; 24]),
        )),
        Some(40),
    )
    .interior_lighting(id(31));
    assert!(result.is_err(), "nonfinite optional light fade is invalid");
}
