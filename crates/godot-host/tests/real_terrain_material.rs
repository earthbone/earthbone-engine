// SPDX-License-Identifier: GPL-3.0-or-later

//! Terrain footstep material on vanilla Tamriel. LAND collision has no Havok
//! material, so `DataSource::terrain_material` resolves the painted LTEX and
//! its `MNAM`. Gated on `SKYRIM_DATA`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{GlobalFormId, Tag};
use soe_godot_host::data_source::DataSource;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

#[test]
fn tamriel_terrain_resolves_painted_materials() {
    let Some(data) = data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}"));
    let tamriel = GlobalFormId {
        file: 0,
        object: 0x3c,
    };
    let order = source.shared_order();
    let mut resolved = 0;
    let mut distinct = std::collections::BTreeSet::new();
    // The Whiterun hold plains, cells 0 to 7 east and minus 4 to 3 north,
    // sampled every 512 units.
    for step_x in 0..64 {
        for step_y in 0..64 {
            let x = step_x as f32 * 512.0 + 7.0;
            let y = -16384.0 + step_y as f32 * 512.0 + 7.0;
            let Some(material) = source.terrain_material(tamriel, x, y) else {
                continue;
            };
            let record = order
                .winner_ref(material)
                .map(|found| order.record(found).tag);
            assert_eq!(
                record,
                Some(Tag::new(b"MATT")),
                "terrain at {x},{y} named {material}, which is not a MATT"
            );
            resolved += 1;
            distinct.insert(material);
        }
    }
    eprintln!(
        "terrain materials: {resolved} of 4096 samples resolved, {} distinct",
        distinct.len()
    );
    assert!(
        resolved > 2048,
        "most of the plains must name a painted material, got {resolved}"
    );
    assert!(
        distinct.len() > 1,
        "the plains mix grass, dirt and rock, one material means the lookup is stuck"
    );
}
