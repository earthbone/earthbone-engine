// SPDX-License-Identifier: GPL-3.0-or-later

//! DDS cubemap faces and authored masks through the production cell loader.

use super::{fixture, DataSource, TextureData};
use soe_dds::header::{caps, flags, pf, Header, PixelFormat};
use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
use soe_nif::{
    blocks::{flags1, flags2},
    vertex::attr,
};
use std::path::Path;

const FACE_COLORS: [u16; 6] = [0xf800, 0x07e0, 0x001f, 0xffe0, 0xf81f, 0x07ff];

fn header(cube: bool) -> Header {
    Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT,
        height: if cube { 4 } else { 1 },
        width: if cube { 4 } else { 8 },
        pitch_or_linear_size: 0,
        depth: 0,
        mip_map_count: 1,
        pixel_format: if cube {
            PixelFormat {
                flags: pf::FOURCC,
                four_cc: *b"DXT1",
                rgb_bit_count: 0,
                r_mask: 0,
                g_mask: 0,
                b_mask: 0,
                a_mask: 0,
            }
        } else {
            PixelFormat {
                flags: pf::RGB | pf::ALPHAPIXELS,
                four_cc: [0; 4],
                rgb_bit_count: 32,
                r_mask: 0xff,
                g_mask: 0xff00,
                b_mask: 0xff0000,
                a_mask: 0xff000000,
            }
        },
        caps: caps::TEXTURE | if cube { caps::COMPLEX } else { 0 },
        caps2: if cube {
            caps::CUBEMAP | caps::CUBEMAP_ALL_FACES
        } else {
            0
        },
        dx10: None,
    }
}

fn cube(colors: [u16; 6]) -> Vec<u8> {
    let mut bytes = header(true).to_bytes();
    for color in colors {
        bytes.extend(color.to_le_bytes());
        bytes.extend([0; 6]);
    }
    bytes
}

fn strip(left: [u8; 4], right: [u8; 4]) -> Vec<u8> {
    let mut bytes = header(false).to_bytes();
    bytes.extend(left.repeat(4));
    bytes.extend(right.repeat(4));
    bytes
}

fn model(index: usize) -> Vec<u8> {
    let face = if index < 6 { index } else { 4 };
    let (u, v, normal) = match face {
        0 => ([0., 1., 0.], [0., 0., 1.], [1., 0., 0.]),
        1 => ([0., -1., 0.], [0., 0., 1.], [-1., 0., 0.]),
        2 => ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
        3 => ([1., 0., 0.], [0., 0., 1.], [0., -1., 0.]),
        4 => ([1., 0., 0.], [0., 1., 0.], [0., 0., 1.]),
        _ => ([-1., 0., 0.], [0., 1., 0.], [0., 0., -1.]),
    };
    let mask = match index {
        6 | 7 | 10 | 13 => "textures/mask.dds",
        12 => "textures/half.dds",
        _ => "",
    };
    let scale = match index {
        7 | 8 | 12 => 0.5,
        10 => 0.0,
        _ => 0.25,
    };
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&[
        "",
        if index == 8 {
            "textures/normal.dds"
        } else {
            ""
        },
        "",
        "",
        if index == 11 {
            "textures/other.dds"
        } else {
            "textures/cube.dds"
        },
        mask,
    ]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(
        1,
        flags1::ZBUFFER_TEST
            | if index == 9 {
                0
            } else {
                flags1::ENVIRONMENT_MAPPING
            }
            | if index == 13 { flags1::DECAL } else { 0 },
        flags2::DOUBLE_SIDED,
        textures,
        &[scale],
    );
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (mut vertices, triangles) = fixture::quad();
    for vertex in &mut vertices {
        let [x, y, _] = vertex.position;
        vertex.position =
            std::array::from_fn(|axis| ((x - 0.5) * u[axis] + (y - 0.5) * v[axis]) * 70.0);
        vertex.normal = normal;
        vertex.tangent = u;
        vertex.bitangent = v;
    }
    let shape = writer.bs_tri_shape(
        "ReflectionQuad",
        [0.; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS | attr::TANGENTS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    let bytes = writer.ni_node("Root", [0.; 3], fixture::IDENTITY, 1., &[shape]);
    writer.fill(root, bytes);
    writer.roots = vec![root];
    writer.finish()
}

fn fixture(directory: &Path) {
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    std::fs::create_dir_all(directory.join("textures")).unwrap();
    for (name, bytes) in [
        ("cube.dds", cube(FACE_COLORS)),
        ("other.dds", cube([0xffe0; 6])),
        ("mask.dds", strip([255, 0, 255, 255], [0, 255, 255, 255])),
        ("half.dds", strip([128, 0, 0, 255], [128, 0, 0, 255])),
        (
            "normal.dds",
            strip([128, 128, 255, 255], [128, 128, 255, 0]),
        ),
    ] {
        std::fs::write(directory.join("textures").join(name), bytes).unwrap();
    }
    let mut plugin = Plugin::new(FileHeader::default());
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    for i in 0..14 {
        let id = 0x100 + i as u32;
        let path = format!("environment{i}.nif");
        std::fs::write(directory.join("meshes").join(&path), model(i)).unwrap();
        let base = plugin.push_record(Record::new(tag::STAT, id).zsub(b"MODL", &path));
        statics.entries.push(Entry::Record(base));
        let cell_id = id + 0x100;
        let cell = plugin.push_record(
            Record::new(tag::CELL, cell_id)
                .zsub(tag::EDID, &format!("Environment{i}"))
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
fn environment_fixture_preserves_cube_faces_masks_and_scale() {
    let exported = std::env::var_os("SOE_ENVIRONMENT_FIXTURE");
    let directory = exported.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("soe-environment-{}", std::process::id())),
        std::path::PathBuf::from,
    );
    fixture(&directory);
    let source = DataSource::open(&directory).unwrap();
    let mut surfaces = Vec::new();
    for i in 0..14 {
        let cell = source.stream_cell(&format!("Environment{i}")).unwrap();
        assert_eq!(cell.surfaces.len(), 1);
        let surface = cell.surfaces.into_iter().next().unwrap();
        let material = &surface.arrays.material;
        assert_eq!(
            material.environment_scale,
            match i {
                7 | 8 | 12 => 0.5,
                10 => 0.,
                _ => 0.25,
            }
        );
        assert_eq!(surface.textures.environment.is_some(), i != 9);
        assert_eq!(
            surface.textures.environment_mask.is_some(),
            matches!(i, 6 | 7 | 10 | 12 | 13)
        );
        if i == 0 {
            let TextureData::Cubemap(faces) = surface.textures.environment.as_deref().unwrap()
            else {
                panic!("cubemap expected");
            };
            for (face, color) in faces.iter().zip(FACE_COLORS) {
                let TextureData::Compressed {
                    bytes,
                    mips,
                    mip0_len,
                    ..
                } = face
                else {
                    panic!("BC1 passthrough expected");
                };
                assert_eq!(*mip0_len, 8);
                assert_eq!(*mips, 3);
                assert_eq!(&bytes[..2], &color.to_le_bytes());
                assert_eq!(bytes.len(), 24);
            }
        }
        surfaces.push(surface);
    }
    for reverse in [false, true] {
        if reverse {
            surfaces.reverse();
        }
        let (batches, _) = crate::batch::batch_surfaces(&surfaces);
        assert_eq!(
            batches.len(),
            9,
            "six directions share a material, eight other states stay separate"
        );
        let keys: std::collections::HashSet<_> = surfaces
            .iter()
            .map(|s| crate::mesh::MaterialKey::from(&s.arrays.material))
            .collect();
        assert_eq!(keys.len(), 9);
    }
    assert_eq!(source.textures.counters().decoded, 5);
    if exported.is_none() {
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn incomplete_environment_cube_is_rejected_before_upload() {
    let mut header = header(true);
    header.caps2 = caps::CUBEMAP | 0x400;
    let mut bytes = header.to_bytes();
    bytes.extend([0; 8]);
    let path = std::env::temp_dir().join(format!("soe-cube-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("broken.dds"), bytes).unwrap();
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(&path).unwrap();
    let cache = super::TextureCache::default();
    let material = crate::mesh::MaterialDesc {
        environment: Some("broken.dds".into()),
        shader_flags_1: flags1::ENVIRONMENT_MAPPING,
        ..Default::default()
    };
    assert!(cache.resolve(&vfs, &material).environment.is_none());
    assert_eq!(cache.counters().failures, 1);
    std::fs::remove_dir_all(path).unwrap();
}
