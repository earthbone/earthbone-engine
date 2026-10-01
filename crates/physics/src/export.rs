// SPDX-License-Identifier: GPL-3.0-or-later

//! Collision export for imported meshes. A mesh node whose name ends in
//! `_collision` is exported as a static `bhkConvexVerticesShape`.

use soe_nif::blocks::{AvObject, Block, Node, ObjectNet};
use soe_nif::{BlockRef, Header, NifFile};

use crate::bhk::UNITS_PER_METER;
use crate::{Error, Result};

/// Imported mesh geometry used by the collision naming convention.
/// Positions use Skyrim game units, matching NIF geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
}

/// Export a named collision mesh as a small NIF block graph.
///
/// Names ending in `_collision` (ASCII case-insensitive) declare collision
/// geometry. Other names return `Ok(None)`. The source positions are converted
/// from game units to Havok metres once. The result has local block references
/// and can be merged into a full NIF by a writer that rebases those references.
///
/// The exported convex shape preserves the source point set. It does not use
/// source triangles because Havok's convex-vertices shape derives its faces
/// from the vertex set.
///
/// # Errors
/// Returns `Error::Invalid` when the marked mesh has an unsupported vertex
/// count or a non-finite coordinate.
///
/// # Panics
/// This function does not panic for caller-provided mesh data.
pub fn export_collision_mesh(mesh: &CollisionMesh) -> Result<Option<NifFile>> {
    if !mesh.name.to_ascii_lowercase().ends_with("_collision") {
        return Ok(None);
    }
    if mesh.positions.len() < 4 || mesh.positions.len() > 1024 {
        return Err(Error::Invalid(
            "convex collision needs 4 to 1024 vertices".into(),
        ));
    }
    if mesh
        .positions
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(Error::Invalid(
            "collision vertex contains a non-finite coordinate".into(),
        ));
    }

    // CONTEXT: Imported positions describe the surface, so no shell is added.
    let mut positions = mesh.positions.clone();
    positions.sort_by(|left, right| {
        left[0]
            .total_cmp(&right[0])
            .then_with(|| left[1].total_cmp(&right[1]))
            .then_with(|| left[2].total_cmp(&right[2]))
    });

    let mut shape = Vec::with_capacity(40 + positions.len() * 16);
    put_u32(&mut shape, 1); // Havok material
    put_f32(&mut shape, 0.0);
    for _ in 0..2 {
        put_u32(&mut shape, 0); // property data
        put_u32(&mut shape, 0); // property size
        put_u32(&mut shape, 0x8000_0000); // capacity and flags
    }
    put_u32(&mut shape, positions.len() as u32);
    for point in &positions {
        for coordinate in point {
            put_f32(&mut shape, coordinate / UNITS_PER_METER);
        }
        put_f32(&mut shape, 0.0);
    }
    put_u32(&mut shape, 0); // optional planes, calculated by Havok

    let mut body = Vec::with_capacity(250);
    put_i32(&mut body, 3); // convex shape block
    body.push(1); // static collision layer
    body.push(0); // filter flags
    put_u16(&mut body, 0); // collision group
    body.resize(body.len() + 44, 0);
    // Identity body transform and the Skyrim rigid-body tail.
    put_vec4(&mut body, [0.0; 3]);
    for value in [0.0, 0.0, 0.0, 1.0] {
        put_f32(&mut body, value);
    }
    put_vec4(&mut body, [0.0; 3]);
    put_vec4(&mut body, [0.0; 3]);
    body.resize(body.len() + 48, 0);
    put_vec4(&mut body, [0.0; 3]);
    for value in [0.0, 0.1, 0.05, 1.0, 1.0, 0.5, 0.0, 0.4, 104.4, 31.57, 0.15] {
        put_f32(&mut body, value);
    }
    body.extend_from_slice(&[7, 1, 1, 1, 0, 0, 0, 0]); // fixed motion and flags
    body.resize(body.len() + 12, 0);
    put_u32(&mut body, 0);
    put_u16(&mut body, 0);

    let mut collision_object = Vec::with_capacity(10);
    put_i32(&mut collision_object, -1); // target is the containing node
    put_u16(&mut collision_object, 1);
    put_i32(&mut collision_object, 2); // rigid body block

    let node = Node {
        av: AvObject {
            net: ObjectNet {
                name: mesh.name.clone(),
                ..ObjectNet::default()
            },
            collision: BlockRef(1),
            ..AvObject::default()
        },
        ..Node::default()
    };

    let mut header = Header {
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
    };
    let blocks = vec![
        Block::Node(node),
        Block::Collision {
            type_name: "bhkCollisionObject".into(),
            data: collision_object,
        },
        Block::Collision {
            type_name: "bhkRigidBodyT".into(),
            data: body,
        },
        Block::Collision {
            type_name: "bhkConvexVerticesShape".into(),
            data: shape,
        },
    ];
    header.block_sizes = vec![0; blocks.len()];
    Ok(Some(NifFile {
        header,
        blocks,
        roots: vec![BlockRef(0)],
    }))
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_vec4(out: &mut Vec<u8>, value: [f32; 3]) {
    for coordinate in value {
        put_f32(out, coordinate);
    }
    put_f32(out, 0.0);
}
