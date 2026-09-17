// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Synthetic files written from nif.xml by `common::NifWriter`, read back
//! through the public API. Each test pins one layout the reader must get
//! byte-exact.

mod common;

use common::{quad, Buf, NifWriter, TestPartition, TestVertex, IDENTITY, ROT_Z_90};
use soe_nif::blocks::{flags1, flags2, BlendFactor, Block, ShaderExtra, ShaderType, TestFunc};
use soe_nif::mesh::MaterialKind;
use soe_nif::vertex::attr;
use soe_nif::{parse, Error, Vec3};

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn assert_vec3(got: [f32; 3], want: [f32; 3], tol: f32) {
    for i in 0..3 {
        assert!(close(got[i], want[i], tol), "got {got:?}, want {want:?}");
    }
}

/// The reference scene: BSMultiBoundNode (a node with trailing fields the
/// reader skips) over a rotated NiNode over one BSTriShape with a lighting
/// shader, a texture set, an alpha property, a Havok block and an extra
/// data block the reader does not decode.
fn reference_scene() -> Vec<u8> {
    let mut w = NifWriter::new(100);
    let (verts, tris) = quad();
    let textures = w.texture_set(&[
        "textures\\clutter\\bucket01.dds",
        "textures\\clutter\\bucket01_n.dds",
    ]);
    let textures = w.add("BSShaderTextureSet", textures);
    let shader = w.lighting_shader(
        1,
        flags1::SPECULAR
            | flags1::CAST_SHADOWS
            | flags1::RECEIVE_SHADOWS
            | flags1::ZBUFFER_TEST
            | flags1::ENVIRONMENT_MAPPING,
        flags2::ZBUFFER_WRITE | flags2::DOUBLE_SIDED | flags2::VERTEX_COLORS,
        textures,
        &[0.5],
    );
    let shader = w.add("BSLightingShaderProperty", shader);
    // Blend on, src alpha, one minus src alpha, test on, greater.
    let alpha_flags: u16 = 1 | (6 << 1) | (7 << 5) | (1 << 9) | (4 << 10);
    let alpha = w.alpha_property(alpha_flags, 128);
    let alpha = w.add("NiAlphaProperty", alpha);
    let collision = w.add("bhkCollisionObject", vec![0xAB; 21]);
    let attrs = attr::VERTEX | attr::UVS | attr::NORMALS | attr::TANGENTS | attr::COLORS;
    let shape = w.bs_tri_shape(
        "Bucket:0", [0.0; 3], -1, shader, alpha, attrs, &verts, &tris, true,
    );
    let shape = w.add("BSTriShape", shape);
    let extra = w.add("NiStringExtraData", vec![0xCD; 9]);
    let sub = w.ni_node("Sub", [0.0; 3], ROT_Z_90, 1.0, &[shape]);
    let sub = w.add("NiNode", sub);
    // BSMultiBoundNode: NiNode fields, then a MultiBound ref and a culling mode.
    let mut root = Buf(w.ni_node("Root", [10.0, 0.0, 0.0], IDENTITY, 2.0, &[sub, collision]));
    root.i32(-1).u32(1);
    let mut root_bytes = root.0;
    // Give the root an extra data entry by rewriting its ObjectNET prefix:
    // name index, then the extra data list.
    let name = w.string("Root");
    let mut head = Buf::default();
    head.u32(name).refs(&[extra]).i32(-1);
    root_bytes.splice(0..12, head.0);
    let root = w.add("BSMultiBoundNode", root_bytes);
    w.roots = vec![root];
    w.finish()
}

#[test]
fn a_bs_tri_shape_scene_loads_to_one_mesh() {
    let bytes = reference_scene();
    let file = parse(&bytes).expect("the reference scene parses");
    assert_eq!(file.header.bs_version, 100);
    assert_eq!(file.blocks.len(), 8);
    assert!(
        matches!(&file.blocks[3], Block::Collision { type_name, data } if type_name == "bhkCollisionObject" && data.len() == 21)
    );
    assert!(
        matches!(&file.blocks[5], Block::Unknown { type_name, .. } if type_name == "NiStringExtraData")
    );
    let root = file.blocks[7]
        .as_node()
        .expect("the multibound node reads as a node");
    assert_eq!(root.av.name(), "Root");
    assert_eq!(root.av.net.extra_data.len(), 1);
    assert_eq!(root.children.len(), 2);

    let meshes = file.meshes().unwrap();
    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert_eq!(m.name, "Bucket:0");
    assert_eq!(m.vertex_count(), 4);
    assert_eq!(m.indices, vec![0, 1, 2, 0, 2, 3]);
    assert_eq!(m.positions[2], [1.0, 1.0, 0.0]);
    assert_eq!(m.uvs[2], [1.0, 1.0]);
    assert_vec3(m.normals[0], [0.0, 0.0, 1.0], 0.01);
    assert_vec3(
        [m.tangents[0][0], m.tangents[0][1], m.tangents[0][2]],
        [1.0, 0.0, 0.0],
        0.01,
    );
    assert_eq!(
        m.tangents[0][3], 1.0,
        "bitangent +Y with normal +Z and tangent +X is right handed"
    );
    assert!(close(m.colors[0][1], 128.0 / 255.0, 1e-6));

    // Root: scale 2 then translate (10, 0, 0). Sub: rotate 90 degrees about Z.
    let world = m.transform.apply(Vec3::new(1.0, 0.0, 0.0));
    assert_vec3(world.to_array(), [10.0, 2.0, 0.0], 1e-5);
    assert_eq!(m.transform.scale, 2.0);

    let mat = &m.material;
    assert_eq!(mat.kind, MaterialKind::Lighting(ShaderType::EnvironmentMap));
    assert_eq!(mat.extra, ShaderExtra::EnvironmentMapScale(0.5));
    assert_eq!(mat.diffuse(), Some("textures\\clutter\\bucket01.dds"));
    assert_eq!(mat.normal(), Some("textures\\clutter\\bucket01_n.dds"));
    assert_eq!(mat.glow(), None);
    assert!(mat.double_sided && mat.vertex_colors && mat.cast_shadows && mat.environment_mapping);
    assert!(!mat.vertex_alpha);
    let blend = mat.blend.expect("blend enabled");
    assert_eq!(blend.src, BlendFactor::SrcAlpha);
    assert_eq!(blend.dst, BlendFactor::OneMinusSrcAlpha);
    let test = mat.alpha_test.expect("alpha test enabled");
    assert_eq!(test.func, TestFunc::Greater);
    assert!(close(test.threshold, 128.0 / 255.0, 1e-6));
}

fn skinned_vertices() -> Vec<TestVertex> {
    let mut v = vec![
        TestVertex::at([0.0, 0.0, 0.0], [0.0, 0.0]),
        TestVertex::at([1.0, 0.0, 0.0], [1.0, 0.0]),
        TestVertex::at([0.0, 1.0, 0.0], [0.0, 1.0]),
    ];
    v[0].weights = [1.0, 0.0, 0.0, 0.0];
    v[0].bone_indices = [0, 0, 0, 0];
    v[1].weights = [0.75, 0.25, 0.0, 0.0];
    v[1].bone_indices = [1, 0, 0, 0];
    v[2].weights = [0.5, 0.5, 0.0, 0.0];
    v[2].bone_indices = [0, 1, 0, 0];
    v
}

#[test]
fn a_skyrim_le_tri_shape_with_a_partition_maps_bones_through_the_partition() {
    let mut w = NifWriter::new(83);
    let verts = skinned_vertices();
    let tris = vec![[0u16, 1, 2]];
    let bone0 = w.ni_node("Bone0", [0.0; 3], IDENTITY, 1.0, &[]);
    let bone0 = w.add("NiNode", bone0);
    let bone1 = w.ni_node("Bone1", [5.0, 0.0, 0.0], IDENTITY, 1.0, &[]);
    let bone1 = w.add("NiNode", bone1);
    let data = w.geometry_data(&verts, true, &tris, None);
    let data = w.add("NiTriShapeData", data);
    let skin_data = w.skin_data(
        &[([0.0, 0.0, -1.0], vec![]), ([-5.0, 0.0, 0.0], vec![])],
        false,
    );
    let skin_data = w.add("NiSkinData", skin_data);
    // The partition lists the bones in reverse order so local index 0 is
    // skin bone 1.
    let partition = w.skin_partition(
        &[TestPartition {
            bones: vec![1, 0],
            vertex_map: vec![2, 0, 1],
            bones_per_vertex: 4,
            weights: vec![0.5, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.25, 0.75, 0.0, 0.0],
            bone_indices: vec![0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0],
            triangles: vec![[1, 2, 0]],
            triangles_copy: vec![],
        }],
        None,
    );
    let partition = w.add("NiSkinPartition", partition);
    let root = w.reserve("NiNode");
    let instance = w.skin_instance(skin_data, partition, root, &[bone0, bone1], None);
    let instance = w.add("NiSkinInstance", instance);
    let shape = w.ni_tri_shape("Body:0", data, instance, -1, -1);
    let shape = w.add("NiTriShape", shape);
    let root_bytes = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[bone0, bone1, shape]);
    w.fill(root, root_bytes);
    w.roots = vec![root];
    let bytes = w.finish();

    let file = parse(&bytes).unwrap();
    let meshes = file.meshes().unwrap();
    assert_eq!(meshes.len(), 1);
    let m = &meshes[0];
    assert_eq!(m.indices, vec![0, 1, 2]);
    assert_eq!(m.tangents.len(), 3);
    assert_eq!(m.uvs[1], [1.0, 0.0]);
    let skin = m.skin.as_ref().expect("skinned");
    assert_eq!(skin.skeleton_root_name, "Root");
    assert_eq!(skin.bones.len(), 2);
    assert_eq!(skin.bones[1].name, "Bone1");
    assert_eq!(
        skin.bones[1].inverse_bind.translation,
        Vec3::new(-5.0, 0.0, 0.0)
    );
    // Shape vertex 2 is partition-local 0: bones (local 0, local 1) = (1, 0).
    assert_eq!(m.bone_weights[2], [0.5, 0.5, 0.0, 0.0]);
    assert_eq!(m.bone_indices[2], [1, 0, 0, 0]);
    // Shape vertex 0 is local 1: weight 1.0 on local bone 1 = skin bone 0.
    assert_eq!(m.bone_weights[0], [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(m.bone_indices[0], [0, 0, 0, 0]);
    // Shape vertex 1 is local 2: 0.25 on local 0 (bone 1), 0.75 on local 1 (bone 0).
    assert_eq!(m.bone_weights[1], [0.25, 0.75, 0.0, 0.0]);
    assert_eq!(m.bone_indices[1], [1, 0, 0, 0]);
}

#[test]
fn skin_data_weight_lists_are_used_when_the_partition_has_none() {
    let mut w = NifWriter::new(83);
    let verts = skinned_vertices();
    let bone0 = w.ni_node("Bone0", [0.0; 3], IDENTITY, 1.0, &[]);
    let bone0 = w.add("NiNode", bone0);
    let data = w.geometry_data(&verts, false, &[[0, 1, 2]], None);
    let data = w.add("NiTriShapeData", data);
    let skin_data = w.skin_data(&[([0.0; 3], vec![(0, 1.0), (2, 0.5)])], true);
    let skin_data = w.add("NiSkinData", skin_data);
    let instance = w.skin_instance(skin_data, -1, -1, &[bone0], None);
    let instance = w.add("NiSkinInstance", instance);
    let shape = w.ni_tri_shape("Body:0", data, instance, -1, -1);
    let shape = w.add("NiTriShape", shape);
    let root = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[bone0, shape]);
    let root = w.add("NiNode", root);
    w.roots = vec![root];
    let file = parse(&w.finish()).unwrap();
    let m = &file.meshes().unwrap()[0];
    assert_eq!(m.bone_weights[0], [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(m.bone_weights[1], [0.0; 4]);
    assert_eq!(m.bone_weights[2], [0.5, 0.0, 0.0, 0.0]);
}

#[test]
fn tri_strips_unroll_with_alternating_winding() {
    let mut w = NifWriter::new(100);
    let (verts, _) = quad();
    let data = w.geometry_data(&verts, false, &[], Some(&[vec![0, 1, 3, 2], vec![1, 1, 2]]));
    let data = w.add("NiTriStripsData", data);
    let shape = w.ni_tri_shape("Strip:0", data, -1, -1, -1);
    let shape = w.add("NiTriStrips", shape);
    w.roots = vec![shape];
    let file = parse(&w.finish()).unwrap();
    let m = &file.meshes().unwrap()[0];
    // Strip 0,1,3,2 gives (0,1,3) and the flipped (3,1,2). Strip 1,1,2 is
    // degenerate and drops out.
    assert_eq!(m.indices, vec![0, 1, 3, 3, 1, 2]);
    assert!(matches!(&file.blocks[0], Block::GeometryData(g) if g.from_strips));
}

#[test]
fn a_skinned_sse_shape_reads_its_vertices_from_the_partition() {
    let mut w = NifWriter::new(100);
    let verts = skinned_vertices();
    let attrs = attr::VERTEX | attr::UVS | attr::NORMALS | attr::TANGENTS | attr::SKINNED;
    let bone0 = w.ni_node("NPC Spine", [0.0; 3], IDENTITY, 1.0, &[]);
    let bone0 = w.add("NiNode", bone0);
    let bone1 = w.ni_node("NPC Head", [0.0, 0.0, 10.0], IDENTITY, 1.0, &[]);
    let bone1 = w.add("NiNode", bone1);
    let skin_data = w.skin_data(&[([0.0; 3], vec![]), ([0.0, 0.0, -10.0], vec![])], false);
    let skin_data = w.add("NiSkinData", skin_data);
    let partition = w.skin_partition(
        &[TestPartition {
            bones: vec![0, 1],
            vertex_map: vec![0, 1, 2],
            bones_per_vertex: 4,
            weights: vec![],
            bone_indices: vec![],
            triangles: vec![[0, 1, 2]],
            triangles_copy: vec![[0, 1, 2]],
        }],
        Some((attrs, &verts)),
    );
    let partition = w.add("NiSkinPartition", partition);
    let root = w.reserve("NiNode");
    let instance = w.skin_instance(
        skin_data,
        partition,
        root,
        &[bone0, bone1],
        Some(&[(257, 32)]),
    );
    let instance = w.add("BSDismemberSkinInstance", instance);
    let shape = w.bs_tri_shape(
        "MaleBody:0",
        [0.0; 3],
        instance,
        -1,
        -1,
        attrs,
        &verts,
        &[[0, 1, 2]],
        false,
    );
    let shape = w.add("BSTriShape", shape);
    let root_bytes = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[bone0, bone1, shape]);
    w.fill(root, root_bytes);
    w.roots = vec![root];
    let bytes = w.finish();

    let file = parse(&bytes).unwrap();
    match &file.blocks[shape as usize] {
        Block::BsTriShape(t) => {
            assert!(t.vertices.is_empty(), "the shape itself carries no data");
            assert_eq!(t.num_vertices, 3);
        }
        other => panic!("expected BSTriShape, got {}", other.type_name()),
    }
    let m = &file.meshes().unwrap()[0];
    assert_eq!(m.vertex_count(), 3);
    assert_eq!(m.positions[1], [1.0, 0.0, 0.0]);
    assert_eq!(m.uvs[2], [0.0, 1.0]);
    assert_eq!(m.indices, vec![0, 1, 2]);
    assert_eq!(m.bone_weights[1], [0.75, 0.25, 0.0, 0.0]);
    assert_eq!(m.bone_indices[1], [1, 0, 0, 0]);
    let skin = m.skin.as_ref().unwrap();
    assert_eq!(skin.bones[1].name, "NPC Head");
    assert_eq!(skin.body_parts.len(), 1);
    assert_eq!(skin.body_parts[0].part, 32);
    assert_eq!(skin.bones[1].inverse_bind.translation.z, -10.0);
}

#[test]
fn an_effect_shader_becomes_an_effect_material() {
    let mut w = NifWriter::new(100);
    let (verts, tris) = quad();
    let shader = w.effect_shader(
        flags1::USE_FALLOFF | flags1::SOFT_EFFECT,
        flags2::DOUBLE_SIDED,
        "textures\\effects\\glow.dds",
        [1.0, 0.5, 0.25, 1.0],
    );
    let shader = w.add("BSEffectShaderProperty", shader);
    let shape = w.bs_tri_shape(
        "Glow:0",
        [0.0; 3],
        -1,
        shader,
        -1,
        attr::VERTEX | attr::UVS,
        &verts,
        &tris,
        true,
    );
    let shape = w.add("BSTriShape", shape);
    w.roots = vec![shape];
    let file = parse(&w.finish()).unwrap();
    let m = &file.meshes().unwrap()[0];
    assert!(m.normals.is_empty() && m.tangents.is_empty() && m.colors.is_empty());
    assert_eq!(m.material.kind, MaterialKind::Effect);
    assert_eq!(m.material.diffuse(), Some("textures\\effects\\glow.dds"));
    assert!(m.material.double_sided);
    let e = m.material.effect.as_ref().unwrap();
    assert_eq!(e.base_color, [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(e.base_color_scale, 2.0);
    assert_eq!(e.falloff_stop_opacity, 0.25);
    assert_eq!(e.greyscale_texture, "textures\\effects\\grey.dds");
    assert_eq!(e.lighting_influence, 1.0);
}

#[test]
fn fallout4_geometry_loads_with_half_positions() {
    let mut w = NifWriter::new(130);
    let (verts, tris) = quad();
    let textures = w.texture_set(&["textures\\a.dds"]);
    let textures = w.add("BSShaderTextureSet", textures);
    let shader = w.lighting_shader(0, 0, 0, textures, &[]);
    let shader = w.add("BSLightingShaderProperty", shader);
    let attrs = attr::VERTEX | attr::UVS | attr::NORMALS;
    let shape = w.bs_tri_shape(
        "Fo4:0", [0.0; 3], -1, shader, -1, attrs, &verts, &tris, true,
    );
    let shape = w.add("BSTriShape", shape);
    w.roots = vec![shape];
    let file = parse(&w.finish()).unwrap();
    let m = &file.meshes().unwrap()[0];
    assert_eq!(m.positions[2], [1.0, 1.0, 0.0]);
    assert_eq!(m.material.diffuse(), Some("textures\\a.dds"));
    assert_eq!(m.material.kind, MaterialKind::Lighting(ShaderType::Default));
}

#[test]
fn truncated_input_is_an_error_and_never_a_panic() {
    let bytes = reference_scene();
    for len in (0..bytes.len()).step_by(3) {
        let r = parse(&bytes[..len]);
        assert!(
            r.is_err(),
            "a {len}-byte prefix of a {}-byte file parsed",
            bytes.len()
        );
    }
    assert!(matches!(parse(&bytes[..10]), Err(Error::BadMagic)));
    assert!(matches!(parse(&bytes[..60]), Err(Error::Truncated { .. })));
}

#[test]
fn random_corruption_never_panics() {
    let base = reference_scene();
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut parsed = 0;
    for _ in 0..2000 {
        let mut bytes = base.clone();
        let flips = 1 + (next() % 4) as usize;
        for _ in 0..flips {
            let i = (next() as usize) % bytes.len();
            let bit = next() % 8;
            bytes[i] ^= 1 << bit;
        }
        if let Ok(file) = parse(&bytes) {
            parsed += 1;
            // The mesh walk must also survive whatever the blocks now say.
            let _ = file.meshes();
        }
    }
    // Some flips land in string bytes or padding and still parse.
    assert!(
        parsed > 0,
        "no corrupted variant parsed, the test is not exercising the block readers"
    );
}

#[test]
fn hostile_counts_are_rejected_before_allocation() {
    let mut bytes = reference_scene();
    // The block count sits right after the user version.
    let magic = b"Gamebryo File Format, Version 20.2.0.7\n".len();
    let count_at = magic + 4 + 1 + 4;
    bytes[count_at..count_at + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    assert!(matches!(parse(&bytes), Err(Error::CountTooLarge { .. })));
}

#[test]
fn a_wrong_block_size_is_reported_with_the_block() {
    let mut w = NifWriter::new(100);
    let (verts, tris) = quad();
    let mut shape = w.bs_tri_shape(
        "S:0",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX,
        &verts,
        &tris,
        true,
    );
    shape.push(0);
    let shape = w.add("BSTriShape", shape);
    w.roots = vec![shape];
    match parse(&w.finish()) {
        Err(Error::BlockSize {
            index: 0,
            type_name,
            ..
        }) => assert_eq!(type_name, "BSTriShape"),
        other => panic!("expected a block size error, got {other:?}"),
    }
}

#[test]
fn a_vertex_descriptor_that_lies_about_its_size_is_rejected() {
    let mut w = NifWriter::new(100);
    let (verts, tris) = quad();
    let mut shape = w.bs_tri_shape(
        "S:0",
        [0.0; 3],
        -1,
        -1,
        -1,
        attr::VERTEX | attr::UVS,
        &verts,
        &tris,
        true,
    );
    // The descriptor's low nibble is the size in dwords. Position 4 of the
    // descriptor is after ObjectNET (12) + flags (4) + transform (52) +
    // collision (4) + bound (16) + three refs (12).
    let desc_at = 12 + 4 + 52 + 4 + 16 + 12;
    shape[desc_at] = (shape[desc_at] & 0xF0) | 0x3;
    let shape = w.add("BSTriShape", shape);
    w.roots = vec![shape];
    assert!(matches!(
        parse(&w.finish()),
        Err(Error::InvalidData { block: 0, .. })
    ));
}

#[test]
fn cycles_and_dangling_references_are_errors_not_hangs() {
    let mut w = NifWriter::new(100);
    let node = w.ni_node("Loop", [0.0; 3], IDENTITY, 1.0, &[0]);
    w.add("NiNode", node);
    w.roots = vec![0];
    let file = parse(&w.finish()).unwrap();
    assert!(matches!(file.meshes(), Err(Error::Cycle { block: 0 })));

    let mut w = NifWriter::new(100);
    let node = w.ni_node("Dangling", [0.0; 3], IDENTITY, 1.0, &[99]);
    w.add("NiNode", node);
    w.roots = vec![0];
    let file = parse(&w.finish()).unwrap();
    assert!(matches!(
        file.meshes(),
        Err(Error::BadRef { reference: 99, .. })
    ));

    let mut w = NifWriter::new(100);
    let node = w.ni_node("Root", [0.0; 3], IDENTITY, 1.0, &[]);
    w.add("NiNode", node);
    w.roots = vec![7];
    assert!(matches!(
        parse(&w.finish()),
        Err(Error::BadRef { reference: 7, .. })
    ));
}

#[test]
fn unsupported_streams_are_named() {
    let w = NifWriter::new(34);
    let bytes = w.finish();
    assert!(matches!(
        parse(&bytes),
        Err(Error::UnsupportedVersion { bs_version: 34, .. })
    ));
}
