// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-01's installed-data check: a vanilla exterior cell with LAND alpha
//! layers carries a baked, non-white albedo texture on its terrain surface.
//! The Godot host sends that texture through its existing StandardMaterial3D
//! path. Run with `SKYRIM_DATA` set to an installed Skyrim SE Data directory.

use std::collections::HashSet;
use std::path::PathBuf;

use soe_esm::records::land::Land;
use soe_esm::LoadOrder;
use soe_godot_host::data_source::{DataSource, TextureData};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|error| panic!("DataSource::open: {error}")))
}

fn has_painted_layers(order: &LoadOrder, form: soe_esm::GlobalFormId) -> bool {
    let Some(record) = order.winner(form) else {
        return false;
    };
    Land::from_record(record).is_ok_and(|land| {
        land.quadrants
            .iter()
            .any(|quadrant| quadrant.layers.iter().any(|layer| !layer.alphas.is_empty()))
    })
}

#[test]
fn rnd_01_vanilla_exterior_terrain_has_blended_texture() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();
    let exterior = source
        .stream_exterior("WhiterunOrigin", 1)
        .unwrap_or_else(|error| panic!("stream_exterior: {error}"));
    let mut checked = HashSet::new();

    for (_, cell) in &exterior.cells {
        for surface in &cell.surfaces {
            if !surface
                .arrays
                .material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.starts_with("terrain:"))
                || !has_painted_layers(&order, surface.reference)
                || !checked.insert(surface.reference)
            {
                continue;
            }
            let Some(TextureData::Rgba8(image)) = surface.textures.diffuse.as_deref() else {
                panic!(
                    "LAND {} has no baked RGBA8 diffuse texture",
                    surface.reference
                );
            };
            let colors: HashSet<[u8; 3]> = image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            assert!(
                colors.len() > 1,
                "LAND {} alpha layers did not change its base texture",
                surface.reference
            );
            assert!(
                colors.iter().any(|color| *color != [255, 255, 255]),
                "LAND {} still has an all-white terrain texture",
                surface.reference
            );
            println!(
                "LAND {} texture {}x{}, {} sampled colors",
                surface.reference,
                image.width,
                image.height,
                colors.len()
            );
            return;
        }
    }
    panic!("WhiterunOrigin radius 1 had no terrain cell with painted LAND layers");
}
