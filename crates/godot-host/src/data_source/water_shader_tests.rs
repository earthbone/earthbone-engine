// SPDX-License-Identifier: GPL-3.0-or-later

//! Water shader contracts from xEdit WATR and nif.xml BSWaterShaderProperty.

use super::*;

#[test]
fn water_shader_installed_whiterun_channel_normals_resolve() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping installed water shader check");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&directory), 2).unwrap();
    let mut references = Vec::new();
    for (cell, _) in source.cell_index().cells() {
        let world = source
            .cell_index()
            .grid_of(cell)
            .map(|grid| grid.world)
            .or_else(|| source.cell_index().persistent_cell_world(cell));
        if world
            .and_then(|world| source.order.winner(world))
            .and_then(Record::editor_id)
            .as_deref()
            != Some("WhiterunWorld")
        {
            continue;
        }
        for reference in source.cell_index().references(cell) {
            let Some(owner) = source.order.winner_ref(*reference) else {
                continue;
            };
            let record = source.order.record(owner);
            if record.tag != tag::REFR || record.is_deleted() {
                continue;
            }
            let refr = Refr::from_record(record).unwrap();
            let base = source.order.resolve(owner.file, refr.base);
            if source.order.winner(base).is_some_and(|base| {
                base.editor_id()
                    .is_some_and(|name| name.to_ascii_lowercase().starts_with("wrstairsstream"))
                    || super::model_for(base)
                        .is_some_and(|model| model.to_ascii_lowercase().contains("wrstairsstream"))
            }) {
                references.push(*reference);
            }
        }
    }
    references.sort();
    references.dedup();
    assert!(
        !references.is_empty(),
        "installed Whiterun must contain WRStairsStream channels"
    );
    let mut surfaces = 0;
    for reference in references {
        let loaded = source.stream_reference(reference).unwrap();
        for surface in loaded.surfaces {
            let Some(water) = surface.arrays.material.water.as_ref() else {
                continue;
            };
            let mesh = water
                .mesh
                .as_ref()
                .expect("channel keeps its NIF water property");
            assert_ne!(
                mesh.water_flags & 256,
                0,
                "channel uses authored vertex UVs"
            );
            assert_eq!(surface.arrays.uvs.len(), surface.arrays.positions.len());
            assert!(
                water.shading.wind_speed.iter().any(|s| *s != 0.0),
                "channel has authored flow speed"
            );
            for i in 0..3 {
                assert!(
                    water.noise_layers[i].is_some(),
                    "channel names normal layer {i}"
                );
                let texture = surface.textures.water_noise[i]
                    .as_ref()
                    .expect("channel noise resolves through the BSA VFS");
                assert!(texture.payload_bytes() > 0);
            }
            if water.flags & 8 != 0 {
                assert!(
                    surface.textures.water_flow.is_some(),
                    "enabled flow normals resolve"
                );
            }
            eprintln!(
                "water channel {reference}: flags={:#x} WATR={} noise={:?} shading={:?}",
                mesh.water_flags, water.flags, water.noise_layers, water.shading
            );
            surfaces += 1;
        }
    }
    assert!(
        surfaces > 0,
        "channel surfaces are built through the production material path"
    );
    eprintln!("water shader installed: {surfaces} Whiterun channel surfaces, all three normal layers resolved");
}

#[test]
fn water_shader_missing_and_corrupt_noise_never_uses_checker_albedo() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("textures")).unwrap();
    std::fs::write(directory.path().join("textures/broken.dds"), b"bad DDS").unwrap();
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(directory.path()).unwrap();
    let record = Record::new(Tag::new(b"WATR"), 0x800)
        .zsub(b"NAM2", "textures/missing.dds")
        .zsub(b"NAM3", "textures/broken.dds")
        .zsub(b"NAM5", "textures/missing-flow.dds")
        .sub(b"FNAM", [8]);
    let desc = MaterialDesc {
        water: Some(crate::mesh::WaterMaterial {
            mesh: Some(crate::mesh::WaterMesh {
                flags1: 8,
                flags2: 0x21,
                water_flags: 0x1C4,
                uv_offset: [0.0; 2],
                uv_scale: [1.0; 2],
            }),
            ..water_material_from(&record)
        }),
        ..MaterialDesc::default()
    };
    let textures = resolve_material_textures(&vfs, std::slice::from_ref(&desc));
    assert!(textures[0].water_noise.iter().all(Option::is_none));
    assert!(textures[0].water_flow.is_none());
    assert!(
        textures[0].diffuse.is_none(),
        "water fallback never uses the pink checker"
    );
    assert_eq!(
        desc.water.unwrap().opacity,
        255,
        "missing noise preserves the WATR fallback"
    );
}
