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

#[test]
fn r4_installed_light_material_survey() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&directory)).unwrap();
    if let Some(output) = std::env::var_os("SOE_R4_ASSET_FIXTURE") {
        let output = Path::new(&output);
        std::fs::create_dir_all(output).unwrap();
        assert!(
            !std::fs::canonicalize(output)
                .unwrap()
                .starts_with(std::fs::canonicalize(&directory).unwrap()),
            "asset fixture must not overwrite the installed Data directory"
        );
        for name in super::BASE_MASTERS {
            std::fs::write(
                output.join(name),
                Plugin::new(FileHeader::default()).to_bytes(),
            )
            .unwrap();
        }
        for model in [
            "clutter/woodfires/campfire01burning.nif",
            "clutter/woodfires/fireplacewood01burning.nif",
            "clutter/candles/candlehornwall01.nif",
            "clutter/candles/candlehornchandelier01.nif",
            "architecture/whiterun/wrinteriors/wrintwallstr01wina.nif",
        ] {
            let path = format!("meshes/{model}");
            let bytes = source.vfs.read(&path).unwrap();
            let file = soe_nif::parse(&bytes).unwrap();
            let mut textures = std::collections::BTreeSet::new();
            for mesh in file.meshes().unwrap() {
                textures.extend(
                    mesh.material
                        .textures
                        .into_iter()
                        .filter(|path| !path.is_empty()),
                );
            }
            for block in &file.blocks {
                if let soe_nif::blocks::Block::EffectShader(shader) = block {
                    textures.extend(
                        [
                            shader.source_texture.clone(),
                            shader.greyscale_texture.clone(),
                        ]
                        .into_iter()
                        .filter(|path| !path.is_empty()),
                    );
                }
            }
            for texture in textures {
                let path = output.join(soe_bsa::normalize(&texture));
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, source.vfs.read(&texture).unwrap()).unwrap();
            }
            let path = output.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let mut models = std::collections::BTreeSet::new();
    for id in source.cell_index().references(cell) {
        let Some(reference) = source.order.winner_ref(*id) else {
            continue;
        };
        let record = source.order.record(reference);
        if record.tag != tag::REFR {
            continue;
        }
        let refr = soe_esm::records::refr::Refr::from_record(record).unwrap();
        let Some(base) = source
            .order
            .winner(source.order.resolve(reference.file, refr.base))
        else {
            continue;
        };
        let model = super::model_for(base).or_else(|| {
            base.find(soe_esm::Tag::new(b"MODL"))
                .map(soe_esm::cursor::zstring)
        });
        if base.tag == soe_esm::Tag::new(b"LIGH") {
            eprintln!(
                "R4 LIGH {:?} {:?} {:?}",
                base.editor_id(),
                refr.transform,
                base.find(soe_esm::Tag::new(b"DATA"))
            );
        }
        if let Some(model) = model {
            if ["fire", "candle", "brazier", "torch", "wrinterior"]
                .iter()
                .any(|s| model.to_ascii_lowercase().contains(s))
            {
                eprintln!("R4 placement {model} {:?}", refr.transform);
                models.insert(model);
            }
        }
    }
    for model in models {
        let bytes = source.vfs.read(&format!("meshes/{model}")).unwrap();
        let file = soe_nif::parse(&bytes).unwrap();
        for mesh in file.meshes().unwrap() {
            eprintln!("R4 shape {model} {} {:?}", mesh.name, mesh.material);
        }
        for (arrays, textures) in source.item_preview_surfaces(&model).unwrap() {
            eprintln!(
                "R4 texture {model} {:?} diffuse={} glow={}",
                arrays.material.diffuse,
                textures.diffuse.is_some(),
                textures.glow.is_some()
            );
        }
    }
}

#[test]
fn r4_emission_fixture_preserves_diffuse_modulation() {
    let temporary = tempfile::tempdir().unwrap();
    let exported = std::env::var_os("SOE_R4_EMISSION_FIXTURE");
    let directory = exported
        .as_deref()
        .map(Path::new)
        .unwrap_or(temporary.path());
    fixture(directory);
    let mut albedo = glow_dds();
    let start = albedo.len() - 32;
    albedo[start..start + 16].copy_from_slice(&[128_u8, 128, 128, 255].repeat(4));
    albedo[start + 16..].copy_from_slice(&[32_u8, 32, 32, 255].repeat(4));
    std::fs::write(directory.join("textures/albedo.dds"), albedo).unwrap();
    for decal in [false, true] {
        let mut writer = fixture::NifWriter::new(100);
        let textures = writer.texture_set(&["textures/albedo.dds"]);
        let textures = writer.add("BSShaderTextureSet", textures);
        let shader = writer.lighting_shader_emission(
            0,
            flags1::ZBUFFER_TEST | flags1::OWN_EMIT | if decal { flags1::DECAL } else { 0 },
            flags2::DOUBLE_SIDED,
            textures,
            [1.0, 0.0, 0.0, 0.5],
            &[],
        );
        let shader = writer.add("BSLightingShaderProperty", shader);
        let (mut vertices, triangles) = fixture::quad();
        for vertex in &mut vertices {
            for axis in &mut vertex.position {
                *axis *= 70.0;
            }
        }
        let bytes = writer.bs_tri_shape(
            "EmittingSurface",
            [0.0; 3],
            -1,
            shader,
            -1,
            attr::VERTEX | attr::UVS | attr::NORMALS,
            &vertices,
            &triangles,
            true,
        );
        let shape = writer.add("BSTriShape", bytes);
        writer.roots = vec![shape];
        let name = format!("r4-emission-{}.nif", u8::from(decal));
        std::fs::write(directory.join("meshes").join(&name), writer.finish()).unwrap();
        let source = DataSource::open(directory).unwrap();
        let surfaces = source.item_preview_surfaces(&name).unwrap();
        assert_eq!(surfaces.len(), 1);
        assert!(surfaces[0].0.material.emits_light());
        assert!(surfaces[0].1.diffuse.is_some());
    }
}
