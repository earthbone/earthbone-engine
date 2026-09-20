// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Synthetic scenes: bhk blocks written byte by byte from nif.xml, placed
//! in a block list the way `soe_nif` would parse them, then collected and
//! dropped a character on. No file on disk is involved.

use soe_nif::blocks::{Block, Node};
use soe_nif::{BlockRef, Header};
use soe_physics::bhk::{self, Bhk};
use soe_physics::{
    collect_blocks, CharacterSettings, Layer, MotionType, PhysicsWorld, Shape, SimpleWorld, Vec3,
    UNITS_PER_METER,
};

fn header() -> Header {
    Header {
        version: soe_nif::header::VERSION_20_2_0_7,
        user_version: 12,
        bs_version: soe_nif::header::BS_SKYRIM_SE,
        author: String::new(),
        process_script: String::new(),
        export_script: String::new(),
        max_filepath: String::new(),
        block_types: Vec::new(),
        block_type_index: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        groups: Vec::new(),
    }
}

struct W(Vec<u8>);

impl W {
    fn new() -> W {
        W(Vec::new())
    }
    fn u8(&mut self, v: u8) -> &mut W {
        self.0.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut W {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut W {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn i32(&mut self, v: i32) -> &mut W {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut W {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn zeros(&mut self, n: usize) -> &mut W {
        self.0.extend(std::iter::repeat_n(0u8, n));
        self
    }
    fn vec4(&mut self, v: [f32; 3]) -> &mut W {
        self.f32(v[0]).f32(v[1]).f32(v[2]).f32(0.0)
    }
    fn quat_identity(&mut self) -> &mut W {
        self.f32(0.0).f32(0.0).f32(0.0).f32(1.0)
    }
    fn done(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

fn collision_object(body: i32) -> Block {
    let mut w = W::new();
    w.i32(-1).u16(1).i32(body);
    Block::Collision {
        type_name: "bhkCollisionObject".into(),
        data: w.done(),
    }
}

fn simple_phantom(shape: i32) -> Block {
    let mut w = W::new();
    w.i32(shape).u8(30).u8(0x42).u16(0x1234).zeros(28);
    for column in [
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [1.0, 2.0, 3.0, 0.0],
    ] {
        for value in column {
            w.f32(value);
        }
    }
    Block::Collision {
        type_name: "bhkSimpleShapePhantom".into(),
        data: w.done(),
    }
}

fn controller_blocks() -> Vec<Block> {
    let mut capsule = W::new();
    capsule.u32(1591009235).f32(0.25).zeros(8);
    capsule.vec4([0.0, 0.0, 0.25]).vec4([0.0, 0.0, 1.25]);
    vec![
        node("Controller", [10.0, 20.0, 30.0], 1, vec![]),
        collision_object(2),
        simple_phantom(3),
        Block::Collision {
            type_name: "bhkCapsuleShape".into(),
            data: capsule.done(),
        },
    ]
}

#[test]
fn cmb_29_authored_controller_keeps_filter_geometry_and_transform() {
    let out = collect_blocks(&header(), &controller_blocks(), &[BlockRef(0)]).unwrap();
    assert!(
        out.colliders.is_empty(),
        "a phantom must not become a static body"
    );
    assert!(out.skipped.is_empty());
    assert_eq!(out.phantoms.len(), 1);
    let phantom = &out.phantoms[0];
    assert_eq!(phantom.layer, Layer::CHAR_CONTROLLER);
    assert_eq!(phantom.filter_flags, 0x42);
    assert_eq!(phantom.group, 0x1234);
    assert_eq!(
        phantom.transform.translation,
        Vec3::new(10.0, 20.0, 30.0) + Vec3::new(1.0, 2.0, 3.0) * UNITS_PER_METER
    );
    assert_eq!(
        phantom.transform.rotation.rows,
        [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]
    );
    let Shape::Material { id, shape } = &phantom.shape else {
        panic!("missing material");
    };
    assert_eq!(*id, 1591009235);
    let Shape::Capsule { a, b, radius } = shape.as_ref() else {
        panic!("missing capsule");
    };
    assert_eq!(*radius, 0.25 * UNITS_PER_METER);
    assert_eq!(*a, Vec3::new(0.0, 0.0, 0.25) * UNITS_PER_METER);
    assert_eq!(*b, Vec3::new(0.0, 0.0, 1.25) * UNITS_PER_METER);
    let bounds = phantom.shape.aabb(&phantom.transform);
    assert!((bounds.max.z - bounds.min.z - 1.5 * UNITS_PER_METER).abs() < 0.0001);
}

#[test]
fn cmb_29_rejects_truncated_and_nonfinite_controller_data() {
    let blocks = controller_blocks();
    let Block::Collision { type_name, data } = &blocks[2] else {
        panic!("phantom");
    };
    for len in 0..data.len() {
        assert!(
            bhk::decode(type_name, &data[..len], &header()).is_err(),
            "length {len}"
        );
    }
    for (block, offset, value) in [
        (2, 36, f32::NAN),
        (3, 4, f32::NAN),
        (3, 28, -1.0),
        (3, 16, f32::INFINITY),
    ] {
        let mut corrupt = blocks.clone();
        let Block::Collision { data, .. } = &mut corrupt[block] else {
            panic!("collision");
        };
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(collect_blocks(&header(), &corrupt, &[BlockRef(0)]).is_err());
    }
    let mut missing = blocks;
    missing[2] = simple_phantom(99);
    assert!(collect_blocks(&header(), &missing, &[BlockRef(0)]).is_err());
}

#[test]
fn cmb_29_rejects_partial_compound_controller_geometry() {
    let mut blocks = controller_blocks();
    blocks[2] = simple_phantom(4);
    let mut list = W::new();
    list.u32(2).i32(3).i32(99).u32(1591009235).zeros(24).u32(0);
    blocks.push(Block::Collision {
        type_name: "bhkListShape".into(),
        data: list.done(),
    });
    assert!(collect_blocks(&header(), &blocks, &[BlockRef(0)]).is_err());
}

/// A `bhkRigidBodyT` with the full Skyrim SE layout (250 bytes, as the
/// shipped files have it), mass zero, every field the crate does not read
/// left at zero.
fn rigid_body_t(shape: i32, layer: u8, translation: [f32; 3], motion: u8) -> Block {
    rigid_body_with_mass(shape, layer, translation, motion, 0.0)
}

fn rigid_body_with_mass(
    shape: i32,
    layer: u8,
    translation: [f32; 3],
    motion: u8,
    mass: f32,
) -> Block {
    let mut w = W::new();
    w.i32(shape).u8(layer).u8(0).u16(0).zeros(20);
    w.zeros(4).zeros(20);
    w.vec4(translation)
        .quat_identity()
        .vec4([0.0; 3])
        .vec4([0.0; 3]);
    w.zeros(48).vec4([0.0; 3]);
    w.f32(mass)
        .f32(0.1)
        .f32(0.05)
        .f32(1.0)
        .f32(1.0)
        .f32(0.5)
        .f32(0.0)
        .f32(0.4);
    w.f32(104.4).f32(31.57).f32(0.15);
    w.u8(motion)
        .u8(1)
        .u8(1)
        .u8(1)
        .u8(0)
        .u8(0)
        .u8(0)
        .u8(0)
        .zeros(12);
    w.u32(0).u16(0);
    Block::Collision {
        type_name: "bhkRigidBodyT".into(),
        data: w.done(),
    }
}

fn box_shape(half: [f32; 3]) -> Block {
    let mut w = W::new();
    w.u32(1)
        .f32(0.05)
        .zeros(8)
        .f32(half[0])
        .f32(half[1])
        .f32(half[2])
        .f32(0.0);
    Block::Collision {
        type_name: "bhkBoxShape".into(),
        data: w.done(),
    }
}

fn compressed_mesh_shape(data: i32) -> Block {
    let mut w = W::new();
    w.i32(-1)
        .u32(0)
        .f32(0.005)
        .f32(0.0)
        .vec4([1.0; 3])
        .f32(0.005)
        .vec4([1.0; 3])
        .i32(data);
    Block::Collision {
        type_name: "bhkCompressedMeshShape".into(),
        data: w.done(),
    }
}

/// One chunk holding a strip and a triangle list, plus one big triangle.
fn compressed_mesh_data(
    chunk_translation: [f32; 3],
    verts: &[[u16; 3]],
    indices: &[u16],
    strips: &[u16],
    big: (&[[f32; 3]], &[[u16; 3]]),
) -> Block {
    let mut w = W::new();
    w.u32(17).u32(18).u32(0x3ffff).u32(0x1ffff).f32(0.001);
    w.vec4([-100.0; 3]).vec4([100.0; 3]).u8(0).u8(1);
    w.u32(0).u32(0).u32(0);
    w.u32(1).u32(1).u8(1).u8(0).u16(0);
    w.u32(0);
    w.u32(1).vec4([0.0; 3]).quat_identity();
    w.u32(big.0.len() as u32);
    for v in big.0 {
        w.vec4(*v);
    }
    w.u32(big.1.len() as u32);
    for t in big.1 {
        w.u16(t[0]).u16(t[1]).u16(t[2]).u32(0).u16(0);
    }
    w.u32(1);
    w.vec4(chunk_translation).u32(0).u16(0xffff).u16(0);
    w.u32(verts.len() as u32 * 3);
    for v in verts {
        w.u16(v[0]).u16(v[1]).u16(v[2]);
    }
    w.u32(indices.len() as u32);
    for &i in indices {
        w.u16(i);
    }
    w.u32(strips.len() as u32);
    for &s in strips {
        w.u16(s);
    }
    w.u32(0);
    w.u32(0);
    Block::Collision {
        type_name: "bhkCompressedMeshShapeData".into(),
        data: w.done(),
    }
}

fn node(name: &str, translation: [f32; 3], collision: i32, children: Vec<i32>) -> Block {
    let mut n = Node::default();
    n.av.net.name = name.into();
    n.av.transform.translation = soe_nif::Vec3::new(translation[0], translation[1], translation[2]);
    n.av.transform.scale = 1.0;
    n.av.collision = BlockRef(collision);
    n.children = children.into_iter().map(BlockRef).collect();
    Block::Node(n)
}

/// A 20 by 20 metre floor at z = 0 as a compressed mesh, under a root at
/// the origin. Block 0 is the root, 1 the collision object, 2 the body,
/// 3 the shape, 4 the data.
fn floor_blocks() -> Vec<Block> {
    vec![
        node("Floor", [0.0; 3], 1, vec![]),
        collision_object(2),
        rigid_body_t(3, 1, [0.0; 3], 7),
        compressed_mesh_shape(4),
        compressed_mesh_data(
            [-10.0, -10.0, 0.0],
            &[[0, 0, 0], [20000, 0, 0], [20000, 20000, 0], [0, 20000, 0]],
            &[0, 1, 3, 2],
            &[4],
            (&[], &[]),
        ),
    ]
}

#[test]
fn the_floor_collects_to_a_triangle_mesh_in_game_units() {
    let blocks = floor_blocks();
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    assert_eq!(c.colliders.len(), 1);
    let col = &c.colliders[0];
    assert_eq!(col.node_name, "Floor");
    assert_eq!(col.layer, Layer::STATIC);
    assert_eq!(col.motion, MotionType::Fixed);
    assert!(col.is_static());
    let Shape::TriangleMesh {
        vertices,
        triangles,
        materials,
    } = &col.shape
    else {
        panic!("expected a triangle mesh, got {:?}", col.shape);
    };
    assert_eq!(triangles.len(), 2);
    assert_eq!(materials, &[Some(1), Some(1)]);
    let half = 10.0 * UNITS_PER_METER;
    assert!((vertices[0] - Vec3::new(-half, -half, 0.0)).length() < 0.05);
    assert!((vertices[2] - Vec3::new(half, half, 0.0)).length() < 0.05);
    let bb = col.aabb();
    assert!((bb.max.x - half).abs() < 0.05 && bb.min.z.abs() < 1e-3);
}

#[test]
fn a_falling_capsule_stops_on_the_floor() {
    let blocks = floor_blocks();
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    let mut world = SimpleWorld::new();
    world.add_static(&c.colliders[0]);
    let settings = CharacterSettings::default();
    let player = world.create_character(settings, Vec3::new(0.0, 0.0, 300.0));
    for _ in 0..180 {
        world.step(1.0 / 60.0);
    }
    let ch = world.character(player).unwrap();
    assert!(ch.on_ground, "character never touched the floor: {ch:?}");
    assert!(ch.position.z.abs() < 0.5, "feet at {}", ch.position.z);
    assert!(ch.velocity.z.abs() < 1e-3);
    // Walking keeps it on the floor and moves it.
    world.move_character(player, Vec3::new(100.0, 0.0, 0.0));
    for _ in 0..60 {
        world.step(1.0 / 60.0);
    }
    let ch = world.character(player).unwrap();
    assert!(ch.on_ground);
    assert!((ch.position.x - 100.0).abs() < 1.0, "x {}", ch.position.x);
    assert!(ch.position.z.abs() < 0.5);
}

#[test]
fn a_capsule_falls_through_a_floor_that_does_not_block_characters() {
    let mut blocks = floor_blocks();
    blocks[2] = rigid_body_t(3, Layer::WATER.0, [0.0; 3], 7);
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    let mut world = SimpleWorld::new();
    world.add_static(&c.colliders[0]);
    let player = world.create_character(CharacterSettings::default(), Vec3::new(0.0, 0.0, 100.0));
    for _ in 0..120 {
        world.step(1.0 / 60.0);
    }
    let ch = world.character(player).unwrap();
    assert!(!ch.on_ground);
    assert!(ch.position.z < -100.0);
}

#[test]
fn a_box_under_a_moved_node_lands_where_the_transforms_say() {
    // Node at x = 100 game units, body offset half a metre up, box half a
    // metre tall: the top face sits one metre above the node.
    let blocks = vec![
        node("Crate", [100.0, 0.0, 0.0], 1, vec![]),
        collision_object(2),
        rigid_body_t(3, 1, [0.0, 0.0, 0.5], 7),
        box_shape([1.0, 1.0, 0.5]),
    ];
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert_eq!(c.colliders.len(), 1);
    let col = &c.colliders[0];
    let t = col.transform.translation;
    assert!(
        (t - Vec3::new(100.0, 0.0, 0.5 * UNITS_PER_METER)).length() < 1e-3,
        "{t:?}"
    );
    assert_eq!(
        match &col.shape {
            Shape::Material { shape, .. } => shape.as_ref(),
            shape => shape,
        },
        &Shape::Box {
            half_extents: Vec3::new(UNITS_PER_METER, UNITS_PER_METER, 0.5 * UNITS_PER_METER)
        }
    );
    let mut world = SimpleWorld::new();
    world.add_static(col);
    let player = world.create_character(CharacterSettings::default(), Vec3::new(100.0, 0.0, 200.0));
    for _ in 0..180 {
        world.step(1.0 / 60.0);
    }
    let ch = world.character(player).unwrap();
    assert!(ch.on_ground);
    assert!(
        (ch.position.z - UNITS_PER_METER).abs() < 0.5,
        "feet at {}",
        ch.position.z
    );
    // A ray from above finds the top face.
    let hit = world
        .raycast(Vec3::new(100.0, 0.0, 500.0), Vec3::new(100.0, 0.0, -500.0))
        .unwrap();
    assert!((hit.point.z - UNITS_PER_METER).abs() < 1e-2);
    assert!(hit.normal.z > 0.99);
    assert_eq!(hit.material, Some(1));
    // Beside the box there is nothing.
    assert!(world
        .raycast(Vec3::new(300.0, 0.0, 500.0), Vec3::new(300.0, 0.0, -500.0))
        .is_none());
}

#[test]
fn nested_nodes_accumulate_and_a_big_triangle_is_kept() {
    // Root moved by 50, child by 25 more, with a big triangle only.
    let blocks = vec![
        node("Root", [50.0, 0.0, 0.0], -1, vec![1]),
        node("Child", [25.0, 0.0, 0.0], 2, vec![]),
        collision_object(3),
        rigid_body_t(4, 1, [0.0; 3], 7),
        compressed_mesh_shape(5),
        compressed_mesh_data(
            [0.0; 3],
            &[],
            &[],
            &[],
            (
                &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                &[[0, 1, 2]],
            ),
        ),
    ];
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert_eq!(c.colliders.len(), 1);
    assert!((c.colliders[0].transform.translation.x - 75.0).abs() < 1e-4);
    assert_eq!(c.colliders[0].shape.triangle_count(), 1);
    let Shape::TriangleMesh { materials, .. } = &c.colliders[0].shape else {
        panic!("expected a triangle mesh");
    };
    assert_eq!(materials, &[Some(1)]);
}

#[test]
fn unknown_and_dangling_references_are_reported_not_fatal() {
    let blocks = vec![
        node("Odd", [0.0; 3], 1, vec![]),
        collision_object(2),
        rigid_body_t(3, 1, [0.0; 3], 7),
        Block::Collision {
            type_name: "bhkPackedNiTriStripsShape".into(),
            data: vec![0; 40],
        },
    ];
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(c.colliders.is_empty());
    assert_eq!(c.skipped.len(), 1);
    assert!(c.skipped[0].1.contains("bhkPackedNiTriStripsShape"));

    let blocks = vec![node("Dangling", [0.0; 3], 7, vec![])];
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(c.colliders.is_empty() && c.skipped.is_empty());
}

#[test]
fn a_written_body_is_as_long_as_a_shipped_one_and_mass_decides_static() {
    let Block::Collision { data, .. } = rigid_body_t(1, 1, [0.0; 3], 5) else {
        unreachable!()
    };
    assert_eq!(data.len(), 250);
    // Vanilla statics: motion system 5 with mass zero.
    let blocks = vec![
        node("Barrel", [0.0; 3], 1, vec![]),
        collision_object(2),
        rigid_body_with_mass(3, Layer::CLUTTER.0, [0.0; 3], 5, 12.0),
        box_shape([0.3, 0.3, 0.5]),
    ];
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(!c.colliders[0].is_static(), "a 12 kg clutter box moves");
    let mut blocks = blocks;
    blocks[2] = rigid_body_with_mass(3, Layer::STATIC.0, [0.0; 3], 5, 0.0);
    let c = collect_blocks(&header(), &blocks, &[BlockRef(0)]).unwrap();
    assert!(c.colliders[0].is_static());
}

#[test]
fn decoding_a_written_rigid_body_reads_back_its_fields() {
    let Block::Collision { type_name, data } = rigid_body_t(9, 13, [1.0, 2.0, 3.0], 6) else {
        unreachable!()
    };
    let Bhk::RigidBody(b) = bhk::decode(&type_name, &data, &header()).unwrap() else {
        panic!("not a rigid body")
    };
    assert_eq!(b.shape, BlockRef(9));
    assert_eq!(b.layer, Layer::TERRAIN);
    assert_eq!(b.translation, Vec3::new(1.0, 2.0, 3.0));
    assert!((b.rotation.norm() - 1.0).abs() < 1e-6);
    assert_eq!(b.motion, MotionType::Keyframed);
    assert!(b.transformed);
    assert!((b.friction - 0.5).abs() < 1e-6);
}

#[test]
fn every_truncation_of_every_block_is_an_error_or_a_shorter_prefix_never_a_panic() {
    // A decoder reads a prefix of its block (the rigid body stops at the
    // motion type), so a cut past that prefix still decodes. What must hold
    // is that no cut panics and that once a cut decodes, every longer cut
    // does too.
    for block in floor_blocks().into_iter().chain([box_shape([1.0; 3])]) {
        let Block::Collision { type_name, data } = block else {
            continue;
        };
        let mut decoded_once = false;
        for cut in 0..=data.len() {
            let ok = bhk::decode(&type_name, &data[..cut], &header()).is_ok();
            assert!(
                ok || !decoded_once,
                "{type_name} decoded at a shorter cut but failed at {cut} bytes"
            );
            decoded_once |= ok;
        }
        assert!(decoded_once, "{type_name} never decoded from its own bytes");
    }
}
