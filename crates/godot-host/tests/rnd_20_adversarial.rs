// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../nif/tests/common/mod.rs"]
mod nif_fixture;

use soe_esm::{tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, Plugin, Record};
use soe_godot_host::data_source::{DataSource, TextureData};
use std::path::{Path, PathBuf};

fn fallback_nif(decal: bool) -> Vec<u8> {
    use nif_fixture::NifWriter;
    use soe_nif::blocks::{flags1, flags2};
    use soe_nif::vertex::attr;

    let mut writer = NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let texture_set = writer.texture_set(&["textures/rnd20_absent.dds"]);
    let texture_set = writer.add("BSShaderTextureSet", texture_set);
    let shader = writer.lighting_shader(
        0,
        flags1::ZBUFFER_TEST | if decal { flags1::DECAL } else { 0 },
        flags2::DOUBLE_SIDED,
        texture_set,
        &[],
    );
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (mut vertices, triangles) = nif_fixture::quad();
    for vertex in &mut vertices {
        for axis in &mut vertex.position {
            *axis *= 70.0;
        }
    }
    let visible = writer.bs_tri_shape(
        "Visible",
        [0.0; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let visible = writer.add("BSTriShape", visible);
    let hidden = writer.bs_tri_shape(
        "HiddenEmitterHelper",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let hidden = writer.add("BSTriShape", hidden);
    let root_node = writer.ni_node(
        "Root",
        [0.0; 3],
        nif_fixture::IDENTITY,
        1.0,
        &[visible, hidden],
    );
    writer.fill(root, root_node);
    writer.roots = vec![root];
    writer.finish()
}

fn make_fixture(directory: &Path) {
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    for (index, decal) in [false, true].into_iter().enumerate() {
        std::fs::write(
            directory.join(format!("meshes/fallback{index}.nif")),
            fallback_nif(decal),
        )
        .unwrap();
    }

    let mut plugin = Plugin::new(FileHeader::default());
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    for (index, cell_name) in ["Fallback0", "Fallback1"].into_iter().enumerate() {
        let base_id = 0x100 + index as u32 * 0x20;
        let base = plugin.push_record(
            Record::new(tag::STAT, base_id)
                .zsub(b"EDID", "RND20MissingTexture")
                .zsub(b"MODL", &format!("fallback{index}.nif")),
        );
        statics.entries.push(Entry::Record(base));
        let marker = plugin.push_record(
            Record::new(tag::STAT, base_id + 0x10)
                .with_flags(1 << 23)
                .zsub(b"MODL", &format!("fallback{index}.nif")),
        );
        statics.entries.push(Entry::Record(marker));
        let cell_id = 0x200 + index as u32;
        let cell = plugin.push_record(
            Record::new(tag::CELL, cell_id)
                .zsub(b"EDID", cell_name)
                .sub(b"DATA", [1, 0]),
        );
        cells.entries.push(Entry::Record(cell));
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        let mut references = Group::new(GroupKind::CellTemporary(cell_id));
        references.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::REFR, 0x300 + index as u32)
                    .sub(b"NAME", base_id.to_le_bytes())
                    .sub(b"DATA", [0; 24]),
            ),
        ));
        references.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::REFR, 0x310 + index as u32)
                    .sub(b"NAME", (base_id + 0x10).to_le_bytes())
                    .sub(b"DATA", [0; 24]),
            ),
        ));
        references.entries.push(Entry::Record(
            plugin.push_record(
                Record::new(tag::REFR, 0x320 + index as u32)
                    .sub(b"NAME", base_id.to_le_bytes())
                    .sub(b"DATA", [0; 24])
                    .sub(b"XOCP", [0; 32]),
            ),
        ));
        children.entries.push(Entry::Group(references));
        cells.entries.push(Entry::Group(children));
    }
    plugin.groups.extend([statics, cells]);
    std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
}

#[test]
fn rnd_20_missing_texture_checker_survives_repeated_cell_streams() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = std::env::var_os("SOE_RND_20_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().to_path_buf());
    make_fixture(&directory);
    let source = DataSource::open(&directory).unwrap();

    for pass in 0..2 {
        let cell = source.stream_cell("Fallback0").unwrap();
        assert_eq!(
            cell.surfaces.len(),
            1,
            "markers and shaderless helpers stay hidden"
        );
        let Some(TextureData::Rgba8(image)) = cell.surfaces[0].textures.diffuse.as_deref() else {
            panic!("a missing named diffuse must provide an RGBA checker");
        };
        assert_eq!((image.width, image.height), (8, 8));
        assert_eq!(&image.rgba[..4], &[255, 0, 255, 255]);
        assert_eq!(&image.rgba[(2 * 4)..(3 * 4)], &[0, 0, 0, 255]);
        assert_eq!(cell.stats.textures_missing.len(), usize::from(pass == 0));
        assert_eq!(
            cell.surfaces[0].reference,
            GlobalFormId {
                file: 0,
                object: 0x300
            }
        );
    }
}

#[test]
fn rnd_20_whiterun_gate_has_no_unintended_textureless_surface() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed Whiterun survey");
        return;
    };
    let source = DataSource::open_with_threads(&data, 2).unwrap();
    let order = source.shared_order();
    let mut checked = 0;
    let mut intended_glow = 0;
    for origin in ["WhiterunOrigin", "WhiterunExterior04"] {
        let exterior = source.stream_exterior(origin, 1).unwrap();
        for (_, cell) in exterior.cells {
            assert_eq!(cell.stats.meshes_failed, 0);
            assert!(
                cell.stats.textures_missing.is_empty(),
                "missing vanilla texture"
            );
            for surface in cell.surfaces {
                checked += 1;
                if surface.arrays.material.water.is_some() || surface.textures.diffuse.is_some() {
                    continue;
                }
                let record = order.winner(surface.reference).unwrap();
                let owner = order.winner_ref(surface.reference).unwrap();
                let refr = soe_esm::records::refr::Refr::from_record(order.record(owner)).unwrap();
                let base = order.resolve(owner.file, refr.base);
                assert_eq!(
                    base,
                    GlobalFormId {
                        file: 0,
                        object: 0x03DE33
                    }
                );
                assert_eq!(record.tag, tag::REFR);
                assert_eq!(
                    surface.reference,
                    GlobalFormId {
                        file: 0,
                        object: 0x03DE34
                    }
                );
                intended_glow += 1;
            }
        }
    }
    assert!(checked > 1500, "gate survey was unexpectedly small");
    assert_eq!(intended_glow, 1, "only the documented glow may lack albedo");
}
