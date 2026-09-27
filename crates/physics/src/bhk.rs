// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoding of the `bhk*` Havok blocks that `soe_nif` keeps as opaque
//! bytes. Layouts follow nif.xml for stream version 20.2.0.7 with Bethesda
//! version 83 (Skyrim LE) and 100 (Skyrim SE), cross-checked against
//! OpenMW's `components/nif/physics.cpp`. Fallout 4 files (version 130)
//! use different rigid body and shape layouts and are refused with
//! `Error::Unsupported`, not misread.
//!
//! Everything here stays in the file's units: Havok metres for shapes and
//! rigid body transforms, block references as stored. Converting to game
//! units and walking the scene graph happens in `shape`.

use soe_nif::stream::Stream;
use soe_nif::{BlockRef, Header};

use crate::math::{Iso, Mat3, Quat, Vec3};
use crate::{Error, Result};

/// One Havok metre in game units. Bethesda's exporter uses this factor
/// for every bhk shape, OpenMW keeps the same constant.
pub const UNITS_PER_METER: f32 = 69.991_25;

const VERSION_10_1_0_0: u32 = 0x0A01_0000;

/// Skyrim collision layers, the `Layer` byte of a HavokFilter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer(pub u8);

impl Layer {
    pub const STATIC: Layer = Layer(1);
    pub const ANIM_STATIC: Layer = Layer(2);
    pub const CLUTTER: Layer = Layer(4);
    pub const BIPED: Layer = Layer(8);
    pub const TREES: Layer = Layer(9);
    pub const WATER: Layer = Layer(11);
    pub const TRIGGER: Layer = Layer(12);
    pub const TERRAIN: Layer = Layer(13);
    pub const NON_COLLIDABLE: Layer = Layer(15);
    pub const GROUND: Layer = Layer(17);
    pub const INVISIBLE_WALL: Layer = Layer(27);
    pub const CHAR_CONTROLLER: Layer = Layer(30);

    /// Whether a body on this layer blocks a walking character.
    pub fn blocks_characters(self) -> bool {
        !matches!(
            self,
            Layer::WATER
                | Layer::TRIGGER
                | Layer::NON_COLLIDABLE
                | Layer::CHAR_CONTROLLER
                | Layer(7 | 16 | 18 | 21..=25)
        )
    }
}

/// hkMotionType. Anything but `Fixed` and `Keyframed` moves under
/// simulation, and only `Fixed` is a static for the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionType {
    Dynamic,
    Keyframed,
    Fixed,
    Character,
    Other(u8),
}

impl MotionType {
    fn from_byte(b: u8) -> MotionType {
        match b {
            1..=5 | 8 => MotionType::Dynamic,
            6 => MotionType::Keyframed,
            7 => MotionType::Fixed,
            9 => MotionType::Character,
            other => MotionType::Other(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigidBody {
    pub shape: BlockRef,
    pub layer: Layer,
    pub filter_flags: u8,
    pub group: u16,
    /// Only applied by `bhkRigidBodyT`, kept for both so a reader can see
    /// what the file says.
    pub translation: Vec3,
    pub rotation: Quat,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub motion: MotionType,
    /// `bhkRigidBodyT`: the translation and rotation above are applied to
    /// the shape. Plain `bhkRigidBody` ignores them.
    pub transformed: bool,
}

/// Published `bhkSimpleShapePhantom` layout, in Havok metres.
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleShapePhantom {
    pub shape: BlockRef,
    pub layer: Layer,
    pub filter_flags: u8,
    pub group: u16,
    pub transform: Iso,
}

impl RigidBody {
    /// The body's transform in its node's space, in Havok metres.
    pub fn local_transform(&self) -> Iso {
        if self.transformed {
            Iso::from_parts(self.rotation.to_mat3(), self.translation)
        } else {
            Iso::IDENTITY
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompressedChunk {
    pub translation: Vec3,
    pub material_index: u32,
    pub transform_index: u16,
    /// Quantised positions, metres times a thousand, relative to the chunk
    /// translation.
    pub vertices: Vec<[u16; 3]>,
    pub indices: Vec<u16>,
    /// Lengths of the triangle strips at the front of `indices`. Whatever
    /// follows the strips is a plain triangle list.
    pub strips: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompressedMeshData {
    pub error: f32,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub materials: Vec<(u32, Layer)>,
    /// Per chunk transforms, referenced by `CompressedChunk::transform_index`.
    pub transforms: Vec<Iso>,
    pub big_vertices: Vec<Vec3>,
    /// Triangles too large for a chunk, indexing `big_vertices`, paired with
    /// an index into `materials`. Verified against the shipped SE meshes.
    pub big_triangles: Vec<([u16; 3], u32)>,
    pub chunks: Vec<CompressedChunk>,
}

/// A decoded Havok block. Shape variants keep child references as block
/// indices so `shape::collect` can resolve them against the file.
#[derive(Clone, Debug, PartialEq)]
pub enum Bhk {
    CollisionObject {
        target: BlockRef,
        flags: u16,
        body: BlockRef,
    },
    RigidBody(RigidBody),
    SimpleShapePhantom(SimpleShapePhantom),
    Box {
        material: u32,
        radius: f32,
        half_extents: Vec3,
    },
    Sphere {
        material: u32,
        radius: f32,
    },
    Plane {
        material: u32,
        normal: Vec3,
        distance: f32,
        half_extents: Vec3,
        center: Vec3,
    },
    Capsule {
        material: u32,
        radius: f32,
        first: Vec3,
        second: Vec3,
    },
    Cylinder {
        material: u32,
        shell_radius: f32,
        radius: f32,
        first: Vec3,
        second: Vec3,
    },
    ConvexVertices {
        material: u32,
        radius: f32,
        vertices: Vec<Vec3>,
        /// Half spaces `[nx, ny, nz, d]` with the normal pointing out.
        planes: Vec<[f32; 4]>,
    },
    /// `bhkTransformShape` and `bhkConvexTransformShape` share a layout.
    Transform {
        shape: BlockRef,
        material: u32,
        transform: Iso,
    },
    List {
        children: Vec<BlockRef>,
        material: u32,
    },
    /// `bhkMoppBvTreeShape`: the bounding volume tree itself is Havok's
    /// closed format and is skipped, the wrapped shape is what matters.
    MoppBvTree {
        shape: BlockRef,
        scale: f32,
    },
    CompressedMesh {
        data: BlockRef,
        scale: Vec3,
        radius: f32,
    },
    CompressedMeshData(CompressedMeshData),
    /// `bhkNiTriStripsShape`, whose geometry lives in ordinary NIF
    /// `NiTriStripsData` blocks.
    NiTriStrips {
        material: u32,
        scale: Vec3,
        strips: Vec<BlockRef>,
        filters: Vec<(Layer, u8, u16)>,
    },
    /// A bhk block this crate does not decode: constraints, other phantoms, the
    /// pre-Skyrim packed strips, the Fallout 4 family.
    Other(String),
}

fn check_version(h: &Header) -> Result<()> {
    if h.bs_version != soe_nif::header::BS_SKYRIM_LE
        && h.bs_version != soe_nif::header::BS_SKYRIM_SE
    {
        return Err(Error::Unsupported(format!(
            "bhk blocks for Bethesda version {} are not decoded",
            h.bs_version
        )));
    }
    Ok(())
}

fn vec4(s: &mut Stream<'_>) -> Result<Vec3> {
    let v = Vec3::new(s.f32()?, s.f32()?, s.f32()?);
    s.f32()?;
    Ok(v)
}

fn vec3(s: &mut Stream<'_>) -> Result<Vec3> {
    Ok(Vec3::new(s.f32()?, s.f32()?, s.f32()?))
}

fn quat(s: &mut Stream<'_>) -> Result<Quat> {
    Ok(Quat::new(s.f32()?, s.f32()?, s.f32()?, s.f32()?))
}

fn skip(s: &mut Stream<'_>, n: usize) -> Result<()> {
    s.take(n)?;
    Ok(())
}

/// HavokFilter: layer, flags, group.
fn filter(s: &mut Stream<'_>) -> Result<(Layer, u8, u16)> {
    Ok((Layer(s.u8()?), s.u8()?, s.u16()?))
}

/// Matrix44 as the file stores it, column by column with the translation
/// in the last column.
fn matrix44(s: &mut Stream<'_>) -> Result<Iso> {
    let mut m = [0.0f32; 16];
    for v in m.iter_mut() {
        *v = s.f32()?;
    }
    let rotation = Mat3 {
        rows: [[m[0], m[4], m[8]], [m[1], m[5], m[9]], [m[2], m[6], m[10]]],
    };
    Ok(Iso::from_parts(rotation, Vec3::new(m[12], m[13], m[14])))
}

/// bhkConvexShape prefix: material then the shell radius.
fn convex_prefix(s: &mut Stream<'_>) -> Result<(u32, f32)> {
    let material = s.u32()?;
    let radius = s.f32()?;
    if !radius.is_finite() || radius < 0.0 {
        return Err(Error::Invalid("convex shape radius".into()));
    }
    Ok((material, radius))
}

fn refs(s: &mut Stream<'_>) -> Result<Vec<BlockRef>> {
    let n = s.counted(4)?;
    (0..n).map(|_| s.block_ref().map_err(Error::from)).collect()
}

/// Decode one `Block::Collision` payload by its type name.
pub fn decode(type_name: &str, data: &[u8], header: &Header) -> Result<Bhk> {
    let _span = soe_trace::Span::with_payload("physics.bhk_decode", "physics", data.len() as u64);
    check_version(header)?;
    let mut s = Stream::new(data);
    let s = &mut s;
    let block = match type_name {
        "bhkCollisionObject"
        | "bhkPCollisionObject"
        | "bhkSPCollisionObject"
        | "bhkBlendCollisionObject" => {
            let target = s.block_ref()?;
            let flags = s.u16()?;
            let body = s.block_ref()?;
            Bhk::CollisionObject {
                target,
                flags,
                body,
            }
        }
        "bhkRigidBody" | "bhkRigidBodyT" => {
            Bhk::RigidBody(rigid_body(s, type_name == "bhkRigidBodyT")?)
        }
        "bhkSimpleShapePhantom" => {
            let shape = s.block_ref()?;
            let (layer, filter_flags, group) = filter(s)?;
            skip(s, 28)?;
            let matrix = s.take(64)?;
            let value = |i: usize| {
                f32::from_le_bytes([
                    matrix[i * 4],
                    matrix[i * 4 + 1],
                    matrix[i * 4 + 2],
                    matrix[i * 4 + 3],
                ])
            };
            if !(0..16).all(|i| value(i).is_finite()) {
                return Err(Error::Invalid("phantom matrix is not finite".into()));
            }
            let transform = matrix44(&mut Stream::new(matrix))?;
            Bhk::SimpleShapePhantom(SimpleShapePhantom {
                shape,
                layer,
                filter_flags,
                group,
                transform,
            })
        }
        "bhkBoxShape" => {
            let (material, radius) = convex_prefix(s)?;
            skip(s, 8)?;
            let half_extents = vec3(s)?;
            s.f32()?;
            Bhk::Box {
                material,
                radius,
                half_extents,
            }
        }
        "bhkSphereShape" => {
            let (material, radius) = convex_prefix(s)?;
            Bhk::Sphere { material, radius }
        }
        "bhkPlaneShape" => {
            let material = s.u32()?;
            skip(s, 12)?;
            let normal = vec3(s)?;
            let distance = s.f32()?;
            let half_extents = vec4(s)?;
            let center = vec4(s)?;
            let normal_length = normal.length();
            if !normal.is_finite()
                || !normal_length.is_finite()
                || normal_length <= f32::EPSILON
                || !distance.is_finite()
                || !(distance / normal_length).is_finite()
                || !half_extents.is_finite()
                || half_extents.x < 0.0
                || half_extents.y < 0.0
                || half_extents.z < 0.0
                || !center.is_finite()
                || !(center + half_extents).is_finite()
                || !(center - half_extents).is_finite()
            {
                return Err(Error::Invalid("plane geometry".into()));
            }
            Bhk::Plane {
                material,
                normal,
                distance,
                half_extents,
                center,
            }
        }
        "bhkCylinderShape" => {
            let (material, shell_radius) = convex_prefix(s)?;
            skip(s, 8)?;
            let first = vec4(s)?;
            let second = vec4(s)?;
            let radius = s.f32()?;
            skip(s, 12)?;
            let axis = second - first;
            if !first.is_finite()
                || !second.is_finite()
                || !radius.is_finite()
                || radius <= 0.0
                || !axis.length().is_finite()
                || axis.length() <= f32::EPSILON
                || !(radius + shell_radius).is_finite()
            {
                return Err(Error::Invalid("cylinder geometry".into()));
            }
            Bhk::Cylinder {
                material,
                shell_radius,
                radius,
                first,
                second,
            }
        }
        "bhkCapsuleShape" => {
            let (material, radius) = convex_prefix(s)?;
            skip(s, 8)?;
            let first = vec3(s)?;
            let r1 = s.f32()?;
            let second = vec3(s)?;
            let r2 = s.f32()?;
            if [radius, r1, r2].iter().any(|r| !r.is_finite() || *r < 0.0) {
                return Err(Error::Invalid("capsule radius".into()));
            }
            // The two radii are always equal in Bethesda files. Take the
            // larger so a mismatch errs on the side of blocking.
            Bhk::Capsule {
                material,
                radius: radius.max(r1).max(r2),
                first,
                second,
            }
        }
        "bhkConvexVerticesShape" => {
            let (material, radius) = convex_prefix(s)?;
            skip(s, 24)?;
            let n = s.counted(16)?;
            let mut vertices = Vec::with_capacity(n);
            for _ in 0..n {
                vertices.push(vec4(s)?);
            }
            let n = s.counted(16)?;
            let mut planes = Vec::with_capacity(n);
            for _ in 0..n {
                planes.push([s.f32()?, s.f32()?, s.f32()?, s.f32()?]);
            }
            Bhk::ConvexVertices {
                material,
                radius,
                vertices,
                planes,
            }
        }
        "bhkTransformShape" | "bhkConvexTransformShape" => {
            let shape = s.block_ref()?;
            let material = s.u32()?;
            s.f32()?;
            skip(s, 8)?;
            let transform = matrix44(s)?;
            Bhk::Transform {
                shape,
                material,
                transform,
            }
        }
        "bhkListShape" => {
            let children = refs(s)?;
            let material = s.u32()?;
            Bhk::List { children, material }
        }
        "bhkMoppBvTreeShape" => {
            let shape = s.block_ref()?;
            skip(s, 12)?;
            let scale = s.f32()?;
            Bhk::MoppBvTree { shape, scale }
        }
        "bhkCompressedMeshShape" => {
            s.block_ref()?;
            s.u32()?;
            let radius = s.f32()?;
            s.f32()?;
            let scale = vec4(s)?;
            s.f32()?;
            vec4(s)?;
            let data = s.block_ref()?;
            Bhk::CompressedMesh {
                data,
                scale,
                radius,
            }
        }
        "bhkCompressedMeshShapeData" => Bhk::CompressedMeshData(compressed_mesh_data(s)?),
        "bhkNiTriStripsShape" => {
            let material = s.u32()?;
            let radius = s.f32()?;
            if !radius.is_finite() || radius < 0.0 {
                return Err(Error::Invalid("triangle strip shape radius".into()));
            }
            skip(s, 20)?;
            s.u32()?;
            let scale = if header.version >= VERSION_10_1_0_0 {
                let scale = vec4(s)?;
                if !scale.is_finite() {
                    return Err(Error::Invalid("triangle strip shape scale".into()));
                }
                scale
            } else {
                Vec3::new(1.0, 1.0, 1.0)
            };
            let strips = refs(s)?;
            let n = s.counted(4)?;
            if n != strips.len() {
                return Err(Error::Invalid(format!(
                    "triangle strip shape has {} data blocks and {n} filters",
                    strips.len()
                )));
            }
            let mut filters = Vec::with_capacity(n);
            for _ in 0..n {
                filters.push(filter(s)?);
            }
            Bhk::NiTriStrips {
                material,
                scale,
                strips,
                filters,
            }
        }
        other => Bhk::Other(other.to_string()),
    };
    Ok(block)
}

fn rigid_body(s: &mut Stream<'_>, transformed: bool) -> Result<RigidBody> {
    // bhkWorldObject: shape, filter, hkWorldObjectCinfo.
    let shape = s.block_ref()?;
    let (layer, filter_flags, group) = filter(s)?;
    skip(s, 20)?;
    // bhkEntity: response, unused, contact callback delay.
    skip(s, 4)?;
    // bhkRigidBodyCInfo (2010 layout): unused, filter, unused, unknown,
    // response, unused, delay.
    skip(s, 20)?;
    let translation = vec4(s)?;
    let rotation = quat(s)?;
    vec4(s)?;
    vec4(s)?;
    skip(s, 48)?;
    vec4(s)?;
    let mass = s.f32()?;
    s.f32()?;
    s.f32()?;
    s.f32()?;
    s.f32()?;
    let friction = s.f32()?;
    s.f32()?;
    let restitution = s.f32()?;
    s.f32()?;
    s.f32()?;
    s.f32()?;
    let motion = MotionType::from_byte(s.u8()?);
    Ok(RigidBody {
        shape,
        layer,
        filter_flags,
        group,
        translation,
        rotation,
        mass,
        friction,
        restitution,
        motion,
        transformed,
    })
}

fn compressed_mesh_data(s: &mut Stream<'_>) -> Result<CompressedMeshData> {
    skip(s, 16)?;
    let error = s.f32()?;
    let aabb_min = vec4(s)?;
    let aabb_max = vec4(s)?;
    s.u8()?;
    s.u8()?;
    for _ in 0..3 {
        let n = s.counted(4)?;
        skip(s, n * 4)?;
    }
    let n = s.counted(8)?;
    let mut materials = Vec::with_capacity(n);
    for _ in 0..n {
        let material = s.u32()?;
        let (layer, _, _) = filter(s)?;
        materials.push((material, layer));
    }
    s.u32()?;
    let n = s.counted(32)?;
    let mut transforms = Vec::with_capacity(n);
    for _ in 0..n {
        let t = vec4(s)?;
        let q = quat(s)?;
        transforms.push(Iso::from_parts(q.to_mat3(), t));
    }
    let n = s.counted(16)?;
    let mut big_vertices = Vec::with_capacity(n);
    for _ in 0..n {
        big_vertices.push(vec4(s)?);
    }
    let n = s.counted(12)?;
    let mut big_triangles = Vec::with_capacity(n);
    for _ in 0..n {
        let tri = [s.u16()?, s.u16()?, s.u16()?];
        let material = s.u32()?;
        s.u16()?;
        big_triangles.push((tri, material));
    }
    let n = s.counted(28)?;
    let mut chunks = Vec::with_capacity(n);
    for _ in 0..n {
        let translation = vec4(s)?;
        let material_index = s.u32()?;
        s.u16()?;
        let transform_index = s.u16()?;
        let n = s.counted(2)?;
        if n % 3 != 0 {
            return Err(Error::Invalid(format!(
                "compressed mesh chunk vertex count {n} is not a multiple of three"
            )));
        }
        let mut vertices = Vec::with_capacity(n / 3);
        for _ in 0..n / 3 {
            vertices.push([s.u16()?, s.u16()?, s.u16()?]);
        }
        let n = s.counted(2)?;
        let mut indices = Vec::with_capacity(n);
        for _ in 0..n {
            indices.push(s.u16()?);
        }
        let n = s.counted(2)?;
        let mut strips = Vec::with_capacity(n);
        for _ in 0..n {
            strips.push(s.u16()?);
        }
        let n = s.counted(2)?;
        skip(s, n * 2)?;
        chunks.push(CompressedChunk {
            translation,
            material_index,
            transform_index,
            vertices,
            indices,
            strips,
        });
    }
    Ok(CompressedMeshData {
        error,
        aabb_min,
        aabb_max,
        materials,
        transforms,
        big_vertices,
        big_triangles,
        chunks,
    })
}

impl CompressedChunk {
    /// The chunk's triangles as indices into its own vertex list, with the
    /// strips unrolled. Degenerate strip triangles are dropped, and the
    /// winding alternates the way Havok's strip decoder does.
    pub fn triangles(&self) -> Vec<[u16; 3]> {
        let mut out = Vec::new();
        let mut at = 0usize;
        for &len in &self.strips {
            let len = len as usize;
            if at + len > self.indices.len() {
                break;
            }
            let strip = &self.indices[at..at + len];
            for (i, w) in strip.windows(3).enumerate() {
                let (a, b, c) = if i % 2 == 0 {
                    (w[0], w[1], w[2])
                } else {
                    (w[1], w[0], w[2])
                };
                if a != b && b != c && a != c {
                    out.push([a, b, c]);
                }
            }
            at += len;
        }
        let rest = &self.indices[at.min(self.indices.len())..];
        let mut i = 0;
        while i + 3 <= rest.len() {
            out.push([rest[i], rest[i + 1], rest[i + 2]]);
            i += 3;
        }
        out
    }

    /// Vertex positions in the chunk's own space, metres.
    pub fn positions(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.vertices.iter().map(move |v| {
            self.translation + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) * 0.001
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(bs: u32) -> Header {
        Header {
            version: soe_nif::header::VERSION_20_2_0_7,
            user_version: 12,
            bs_version: bs,
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

    fn f(v: f32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn box_shape_reads_half_extents() {
        let mut b = Vec::new();
        b.extend_from_slice(&3u32.to_le_bytes());
        f(0.05, &mut b);
        b.extend_from_slice(&[0; 8]);
        f(0.5, &mut b);
        f(1.0, &mut b);
        f(2.0, &mut b);
        f(0.0, &mut b);
        let got = decode("bhkBoxShape", &b, &header(100)).unwrap();
        assert_eq!(
            got,
            Bhk::Box {
                material: 3,
                radius: 0.05,
                half_extents: Vec3::new(0.5, 1.0, 2.0)
            }
        );
    }

    #[test]
    fn a_truncated_capsule_is_an_error_not_a_panic() {
        let mut b = Vec::new();
        b.extend_from_slice(&0u32.to_le_bytes());
        f(0.1, &mut b);
        b.extend_from_slice(&[0; 8]);
        f(0.0, &mut b);
        assert!(decode("bhkCapsuleShape", &b, &header(100)).is_err());
    }

    #[test]
    fn fallout_4_is_refused() {
        assert!(matches!(
            decode("bhkBoxShape", &[0; 64], &header(130)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn strips_unroll_with_alternating_winding() {
        let c = CompressedChunk {
            translation: Vec3::ZERO,
            material_index: 0,
            transform_index: 0,
            vertices: vec![[0; 3]; 6],
            indices: vec![0, 1, 2, 3, 4, 5, 0],
            strips: vec![4],
        };
        assert_eq!(c.triangles(), vec![[0, 1, 2], [2, 1, 3], [4, 5, 0]]);
    }

    #[test]
    fn matrix44_puts_translation_in_the_last_column() {
        let mut b = Vec::new();
        let cols = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [3.0, 4.0, 5.0, 1.0],
        ];
        for c in cols {
            for v in c {
                f(v, &mut b);
            }
        }
        let mut s = Stream::new(&b);
        let t = matrix44(&mut s).unwrap();
        assert_eq!(t.translation, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(t.rotation, Mat3::IDENTITY);
    }
}
