// SPDX-License-Identifier: GPL-3.0-or-later

//! Whiterun scene and synthetic material checks through the cell loader, RND-20.

use super::*;

fn assert_albedo(
    source: &DataSource,
    reference: GlobalFormId,
    arrays: &SurfaceArrays,
    textures: &SurfaceTextures,
) {
    if arrays.material.water.is_some() {
        return;
    }
    if textures.diffuse.is_none() {
        let owner = source.order.winner_ref(reference).unwrap();
        let refr = Refr::from_record(source.order.record(owner)).unwrap();
        let base = source.order.resolve(owner.file, refr.base);
        let record = source.order.winner(base).unwrap();
        println!("RND-20 reference={reference} base={base} edid={:?} type={} model={:?} texture_set={:?}", record.editor_id(), record.tag, model_for(record), arrays.material);
        if reference
            == (GlobalFormId {
                file: 0,
                object: 0x04221F,
            })
        {
            assert_eq!(
                base,
                GlobalFormId {
                    file: 0,
                    object: 0x021513
                }
            );
            assert_eq!(record.editor_id().as_deref(), Some("NorLever01"));
            assert_eq!(arrays.material.alpha, AlphaMode::Blend);
            assert_eq!(arrays.material.shader_flags_1, 0x80000000);
            assert_eq!(arrays.material.shader_flags_2, 0x21);
            let bytes = source
                .vfs
                .read(&format!("meshes/{}", model_for(record).unwrap()))
                .unwrap();
            let file = placed_model(&bytes).unwrap();
            let detail = file
                .meshes()
                .unwrap()
                .into_iter()
                .find(|mesh| mesh.name == "RuinsLever:3")
                .unwrap();
            assert_eq!(arrays.positions.len(), detail.positions.len());
            assert!(detail.colors.iter().any(|color| color[0] < 1.0));
            return;
        }
        assert_eq!(
            reference,
            GlobalFormId {
                file: 0,
                object: 0x03DE34
            },
            "undocumented textureless reference"
        );
        assert_eq!(
            base,
            GlobalFormId {
                file: 0,
                object: 0x03DE33
            }
        );
        assert_eq!(arrays.material.alpha, AlphaMode::Blend);
        assert_eq!(arrays.material.shader_flags_1, 0xA0000000);
        assert_eq!(arrays.material.shader_flags_2, 0x40000020);
    }
}

#[test]
fn rnd_20_whiterun_gate_scene_has_albedo_or_documented_intent() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping RND-20 scene survey");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let mut surveyed = 0;
    let mut persistent_surfaces = 0;
    for origin in ["WhiterunOrigin", "WhiterunExterior04"] {
        let block = source.resolve_exterior_block(origin, 1).unwrap();
        assert_eq!(block.cells.len(), 9, "survey must cover both gate blocks");
        if let Some(persistent) = source.cell_index().world_persistent_cell(block.world) {
            for id in source.cell_index().references(persistent) {
                let owner = source.order.winner_ref(*id).unwrap();
                let record = source.order.record(owner);
                if record.tag != tag::REFR || models::is_editor_reference(record) {
                    continue;
                }
                let refr = Refr::from_record(record).unwrap();
                let Some(transform) = refr.transform else {
                    continue;
                };
                let x = (transform.position[0] / crate::units::CELL_UNITS).floor() as i32;
                let y = (transform.position[1] / crate::units::CELL_UNITS).floor() as i32;
                if (x - block.origin.0).abs() > 1 || (y - block.origin.1).abs() > 1 {
                    continue;
                }
                let base = source.order.resolve(owner.file, refr.base);
                let record = source.order.winner(base).unwrap();
                let Some(model) = models::render_model_for(record) else {
                    continue;
                };
                let LoadedStatic::Surfaces { built, .. } =
                    source.load_static_surfaces(&model, &refr, false).unwrap();
                for (arrays, textures) in built {
                    assert_albedo(&source, *id, &arrays, &textures);
                    persistent_surfaces += 1;
                }
            }
        }
        let exterior = source.stream_exterior_block(block).unwrap();
        for (cell, loaded) in exterior.cells {
            assert_eq!(
                loaded.stats.meshes_failed, 0,
                "cell {} dropped meshes",
                cell.id
            );
            assert!(
                loaded.stats.textures_missing.is_empty(),
                "missing vanilla texture"
            );
            for surface in &loaded.surfaces {
                assert_albedo(
                    &source,
                    surface.reference,
                    &surface.arrays,
                    &surface.textures,
                );
            }
            surveyed += loaded.surfaces.len();
        }
    }
    assert!(surveyed > 1500, "too little gate geometry surveyed");
    assert!(
        persistent_surfaces > 10,
        "persistent gate geometry was not surveyed"
    );
    println!("RND-20 scene: 18 grid cells, {surveyed} surfaces, {persistent_surfaces} nearby persistent surfaces");
}

#[test]
fn rnd_20_marker_flags_and_room_portal_occlusion_fields_are_not_drawn() {
    for tag in [
        tag::STAT,
        Tag::new(b"FURN"),
        Tag::new(b"DOOR"),
        Tag::new(b"CONT"),
    ] {
        let record = Record::new(tag, 1)
            .with_flags(1 << 23)
            .zsub(b"MODL", "ordinary.nif");
        assert!(models::render_model_for(&record).is_none());
        if tag == tag::STAT {
            assert_eq!(
                model_for(&record).as_deref(),
                Some("ordinary.nif"),
                "collision model selection must remain available"
            );
        }
    }
    for field in [b"XOCP", b"XORD", b"XRMR", b"XPOD", b"XPTL"] {
        let record = Record::new(tag::REFR, 1).sub(field, [0; 32]);
        assert!(models::is_editor_reference(&record));
    }
    let record = Record::new(tag::REFR, 1).sub(b"XPRM", [0; 32]);
    assert!(
        !models::is_editor_reference(&record),
        "ordinary primitives retain their gameplay meaning"
    );
    let mut primitive = [0_u8; 32];
    primitive[28..].copy_from_slice(&3_u32.to_le_bytes());
    assert!(models::is_editor_reference(
        &Record::new(tag::REFR, 1).sub(b"XPRM", primitive)
    ));
}

#[test]
fn rnd_20_hidden_subtrees_and_shaderless_helpers_preserve_visible_siblings() {
    use soe_nif::blocks::{AvObject, Block, BsTriShape, LightingShader, Node};
    use soe_nif::BlockRef;
    let blocks = vec![
        Block::Node(Node {
            av: AvObject {
                flags: 1,
                ..AvObject::default()
            },
            children: vec![BlockRef(1), BlockRef(0)],
            ..Node::default()
        }),
        Block::BsTriShape(BsTriShape {
            shader: BlockRef(5),
            ..BsTriShape::default()
        }),
        Block::BsTriShape(BsTriShape {
            shader: BlockRef(5),
            ..BsTriShape::default()
        }),
        Block::BsTriShape(BsTriShape {
            shader: BlockRef(-1),
            ..BsTriShape::default()
        }),
        Block::BsTriShape(BsTriShape {
            shader: BlockRef(6),
            ..BsTriShape::default()
        }),
        Block::LightingShader(LightingShader::default()),
        Block::Unknown {
            type_name: "BSWaterShaderProperty".into(),
            data: Vec::new(),
        },
    ];
    assert_eq!(nonvisual_shape_indices(&blocks), HashSet::from([1, 3, 4]));
}

#[test]
fn rnd_20_hidden_damage_stages_select_only_the_intact_range() {
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let textures = writer.texture_set(&[]);
    let textures = writer.add("BSShaderTextureSet", textures);
    let shader = writer.lighting_shader(0, 0, 0, textures, &[]);
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = fixture::quad();
    let mut stages = Vec::new();
    let mut shapes = Vec::new();
    for stage in 0..2_u8 {
        let shape = writer.bs_tri_shape(
            "Geometry",
            [0.0; 3],
            -1,
            shader,
            -1,
            soe_nif::vertex::attr::VERTEX,
            &vertices,
            &triangles,
            true,
        );
        let shape = writer.add("BSTriShape", shape);
        shapes.push(shape as usize);
        let mut node = writer.ni_node("Stage", [0.0; 3], fixture::IDENTITY, 1.0, &[shape]);
        node[12..16].copy_from_slice(&(if stage == 0 { 15_u32 } else { 14 }).to_le_bytes());
        node.extend_from_slice(&[stage, stage, stage]);
        stages.push(writer.add("BSDamageStage", node));
    }
    let node = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &stages);
    writer.fill(root, node);
    writer.roots = vec![root];
    let file = placed_model(&writer.finish()).unwrap();
    assert_eq!(
        nonvisual_shape_indices(&file.blocks),
        HashSet::from([shapes[1]])
    );
    assert_eq!(file.meshes().unwrap().len(), 2);
}

fn fixture_model(decal: bool) -> Vec<u8> {
    use soe_nif::blocks::{flags1, flags2};
    use soe_nif::vertex::attr;
    let mut writer = fixture::NifWriter::new(100);
    let root = writer.reserve("NiNode");
    let set = writer.texture_set(&["textures/missing.dds"]);
    let set = writer.add("BSShaderTextureSet", set);
    let shader = writer.lighting_shader(
        0,
        flags1::ZBUFFER_TEST | if decal { flags1::DECAL } else { 0 },
        flags2::DOUBLE_SIDED,
        set,
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
    let shape = writer.add("BSTriShape", shape);
    let helper = writer.bs_tri_shape(
        "EmitterGeometry",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX | attr::UVS | attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let helper = writer.add("BSTriShape", helper);
    let node = writer.ni_node("Root", [0.0; 3], fixture::IDENTITY, 1.0, &[shape, helper]);
    writer.fill(root, node);
    writer.roots = vec![root];
    writer.finish()
}

#[test]
fn rnd_20_cell_filters_editor_geometry_and_binds_checker_in_both_material_paths() {
    use soe_esm::{Entry, FileHeader, Group, GroupKind, Plugin};
    let temporary = tempfile::tempdir().unwrap();
    let exported = std::env::var_os("SOE_RND_20_FIXTURE");
    let directory = exported
        .as_deref()
        .map(Path::new)
        .unwrap_or(temporary.path());
    std::fs::create_dir_all(directory.join("meshes")).unwrap();
    let mut plugin = Plugin::new(FileHeader::default());
    let mut statics = Group::new(GroupKind::Top(tag::STAT));
    let mut cells = Group::new(GroupKind::Top(tag::CELL));
    for (i, decal) in [false, true].into_iter().enumerate() {
        let base = 0x100 + i as u32;
        let cell_id = base + 0x100;
        let path = format!("fallback{i}.nif");
        std::fs::write(directory.join("meshes").join(&path), fixture_model(decal)).unwrap();
        let record = plugin.push_record(
            Record::new(tag::STAT, base)
                .zsub(b"EDID", "MissingTextureControl")
                .zsub(b"MODL", &path),
        );
        statics.entries.push(Entry::Record(record));
        let marker = plugin.push_record(
            Record::new(tag::STAT, base + 0x10)
                .with_flags(1 << 23)
                .zsub(b"MODL", &path),
        );
        statics.entries.push(Entry::Record(marker));
        let cell = plugin.push_record(
            Record::new(tag::CELL, cell_id)
                .zsub(b"EDID", &format!("Fallback{i}"))
                .sub(b"DATA", [1, 0]),
        );
        cells.entries.push(Entry::Record(cell));
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        let mut references = Group::new(GroupKind::CellTemporary(cell_id));
        for j in 0..6_u32 {
            let record = Record::new(tag::REFR, base + 0x200 + j * 0x10)
                .sub(
                    b"NAME",
                    (if j == 1 { base + 0x10 } else { base }).to_le_bytes(),
                )
                .sub(b"DATA", [0; 24]);
            let record = if j >= 2 {
                record.sub(
                    [b"XOCP", b"XRMR", b"XPOD", b"XPTL"][(j - 2) as usize],
                    [0; 32],
                )
            } else {
                record
            };
            references
                .entries
                .push(Entry::Record(plugin.push_record(record)));
        }
        children.entries.push(Entry::Group(references));
        cells.entries.push(Entry::Group(children));
    }
    plugin.groups.extend([statics, cells]);
    std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
    for name in &BASE_MASTERS[1..] {
        std::fs::write(
            directory.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let source = DataSource::open(directory).unwrap();
    for i in 0..2 {
        let loaded = source.stream_cell(&format!("Fallback{i}")).unwrap();
        assert_eq!(
            loaded.surfaces.len(),
            1,
            "editor/helper geometry reached the renderer"
        );
        assert!(Arc::ptr_eq(
            loaded.surfaces[0].textures.diffuse.as_ref().unwrap(),
            &checker_albedo()
        ));
    }
}
