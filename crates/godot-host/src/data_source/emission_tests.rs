// SPDX-License-Identifier: GPL-3.0-or-later

//! Independent NIF, ESM and DDS writers feed the complete cell material path.

use super::fixture;

use std::path::Path;

use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
use soe_nif::blocks::{flags1, flags2};
use soe_nif::vertex::attr;

use super::{DataSource, TextureData};

fn model(decal: bool, glow: bool, color: [f32; 3], strength: f32) -> Vec<u8> {
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&["", "", "textures\\emission.dds"]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader_emission(
        if glow { 2 } else { 0 },
        flags1::ZBUFFER_TEST | if decal { flags1::DECAL } else { 0 },
        flags2::DOUBLE_SIDED | if glow { flags2::GLOW_MAP } else { 0 },
        textures,
        [color[0], color[1], color[2], strength],
        &[],
    );
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (mut vertices, triangles) = fixture::quad();
    for vertex in &mut vertices {
        for axis in &mut vertex.position {
            *axis *= 70.0;
        }
    }
    let shape = writer.bs_tri_shape(
        "EmissiveQuad",
        [0.0; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let bytes = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &[shape]);
    writer.fill(root, bytes);
    writer.roots = vec![root];
    writer.finish()
}

fn glow_dds() -> Vec<u8> {
    use soe_dds::header::{caps, flags, pf, Header, PixelFormat};
    let header = Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT | flags::PITCH,
        height: 1,
        width: 8,
        pitch_or_linear_size: 32,
        depth: 0,
        mip_map_count: 1,
        pixel_format: PixelFormat {
            flags: pf::RGB | pf::ALPHAPIXELS,
            four_cc: [0; 4],
            rgb_bit_count: 32,
            r_mask: 0xff,
            g_mask: 0xff00,
            b_mask: 0xff0000,
            a_mask: 0xff000000,
        },
        caps: caps::TEXTURE,
        caps2: 0,
        dx10: None,
    };
    let mut bytes = header.to_bytes();
    bytes.extend([255, 255, 255, 255].repeat(4));
    bytes.extend([0, 0, 0, 255].repeat(4));
    bytes
}

fn fixture(directory: &Path) {
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    std::fs::create_dir_all(directory.join("textures")).unwrap();
    std::fs::write(directory.join("textures/emission.dds"), glow_dds()).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    for (i, (decal, glow, color, strength)) in [
        (false, false, [0.0; 3], 1.0),
        (false, true, [1.0, 0.0, 0.0], 0.25),
        (false, true, [1.0, 0.0, 0.0], 0.5),
        (false, false, [1.0, 0.0, 0.0], 0.25),
        (true, false, [0.0; 3], 1.0),
        (true, true, [1.0, 0.0, 0.0], 0.25),
        (true, true, [1.0, 0.0, 0.0], 0.5),
        (true, false, [1.0, 0.0, 0.0], 0.25),
    ]
    .into_iter()
    .enumerate()
    {
        let id = 0x100 + i as u32;
        let path = format!("emission{i}.nif");
        std::fs::write(
            directory.join("meshes").join(&path),
            model(decal, glow, color, strength),
        )
        .unwrap();
        let base = plugin.push_record(Record::new(tag::STAT, id).zsub(b"MODL", &path));
        statics.entries.push(Entry::Record(base));
        let cell_id = id + 0x100;
        let cell = plugin.push_record(
            Record::new(tag::CELL, cell_id)
                .zsub(tag::EDID, &format!("Emission{i}"))
                .sub(b"DATA", [1, 0]),
        );
        let reference = plugin.push_record(
            Record::new(tag::REFR, id + 0x200)
                .sub(b"NAME", id.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(cell_id));
        temporary.entries.push(Entry::Record(reference));
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        children.entries.push(Entry::Group(temporary));
        cells.entries.push(Entry::Record(cell));
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
fn emission_fixture_preserves_authored_material_and_gates_shared_texture_slot() {
    let exported = std::env::var_os("SOE_EMISSION_FIXTURE");
    let directory = exported.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("soe-emission-{}", std::process::id())),
        std::path::PathBuf::from,
    );
    fixture(&directory);
    let source = DataSource::open(&directory).unwrap();
    let mut surfaces = Vec::new();
    for i in 0..8 {
        let cell = source.stream_cell(&format!("Emission{i}")).unwrap();
        assert_eq!(cell.stats.meshes_failed, 0);
        assert_eq!(cell.surfaces.len(), 1);
        let surface = cell.surfaces.into_iter().next().unwrap();
        let material = &surface.arrays.material;
        let variant = i % 4;
        assert_eq!(
            material.emissive_color,
            if variant == 0 {
                [0.0; 3]
            } else {
                [1.0, 0.0, 0.0]
            }
        );
        assert_eq!(material.emissive_multiple, [1.0, 0.25, 0.5, 0.25][variant]);
        assert_eq!(material.decal_depth().is_some(), i >= 4);
        assert_eq!(
            surface.textures.glow.is_some(),
            variant == 1 || variant == 2
        );
        if let Some(texture) = &surface.textures.glow {
            let TextureData::Rgba8(image) = texture.as_ref() else {
                panic!("RGBA fixture");
            };
            assert_eq!(
                image.rgba,
                [[255, 255, 255, 255].repeat(4), [0, 0, 0, 255].repeat(4)].concat()
            );
        }
        surfaces.push(surface);
    }
    for reverse in [false, true] {
        if reverse {
            surfaces.reverse();
        }
        let (batches, _) = crate::batch::batch_surfaces(&surfaces);
        assert_eq!(batches.len(), 8, "emission variants must not merge");
        let keys: std::collections::HashSet<_> = surfaces
            .iter()
            .map(|surface| crate::mesh::MaterialKey::from(&surface.arrays.material))
            .collect();
        assert_eq!(keys.len(), 8, "GPU cache must retain every variant");
    }
    assert_eq!(source.textures.counters().decoded, 1);
    if exported.is_none() {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
