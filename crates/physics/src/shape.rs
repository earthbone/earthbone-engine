// SPDX-License-Identifier: GPL-3.0-or-later

//! From a parsed NIF to colliders in game units. The scene walk mirrors
//! `soe_nif::mesh`: every placed object (node or geometry) may carry a
//! collision object, whose rigid body owns a shape tree. The collider's
//! transform is the object's world transform followed by the rigid body's
//! own offset, and its shape is local to that.

use std::collections::HashSet;

use soe_nif::blocks::{AvObject, Block};
use soe_nif::{BlockRef, Header, NifFile};

use crate::bhk::{self, Bhk, CompressedMeshData, Layer, MotionType, UNITS_PER_METER};
use crate::math::{Aabb, Iso, Vec3};
use crate::{Error, Result};

/// A collision shape in the space of its collider. Lengths are game units.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Axis-aligned box centred on the origin.
    Box {
        half_extents: Vec3,
    },
    Sphere {
        radius: f32,
    },
    /// The capsule's axis runs from `a` to `b`.
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
    ConvexHull {
        points: Vec<Vec3>,
    },
    TriangleMesh {
        vertices: Vec<Vec3>,
        triangles: Vec<[u32; 3]>,
        /// Skyrim Havok material IDs in triangle order. Missing entries are unknown.
        materials: Vec<Option<u32>>,
    },
    /// A material on a primitive or a compound's children without their own material.
    Material {
        id: u32,
        shape: Box<Shape>,
    },
    /// Children with their transforms relative to this shape.
    Compound(Vec<(Iso, Shape)>),
}

impl Shape {
    /// Multiply every length by `s`. Used once to go from Havok metres to
    /// game units.
    pub fn scaled(self, s: f32) -> Shape {
        match self {
            Shape::Box { half_extents } => Shape::Box {
                half_extents: half_extents * s,
            },
            Shape::Sphere { radius } => Shape::Sphere { radius: radius * s },
            Shape::Capsule { a, b, radius } => Shape::Capsule {
                a: a * s,
                b: b * s,
                radius: radius * s,
            },
            Shape::ConvexHull { points } => Shape::ConvexHull {
                points: points.into_iter().map(|p| p * s).collect(),
            },
            Shape::TriangleMesh {
                vertices,
                triangles,
                materials,
            } => Shape::TriangleMesh {
                vertices: vertices.into_iter().map(|p| p * s).collect(),
                triangles,
                materials,
            },
            Shape::Material { id, shape } => Shape::Material {
                id,
                shape: Box::new(shape.scaled(s)),
            },
            Shape::Compound(children) => Shape::Compound(
                children
                    .into_iter()
                    .map(|(t, c)| {
                        let t = Iso {
                            translation: t.translation * s,
                            ..t
                        };
                        (t, c.scaled(s))
                    })
                    .collect(),
            ),
        }
    }

    /// Bounding box of the shape placed by `t`.
    pub fn aabb(&self, t: &Iso) -> Aabb {
        match self {
            Shape::Material { shape, .. } => shape.aabb(t),
            Shape::Box { half_extents } => {
                let h = *half_extents;
                let corners = (0..8).map(|i| {
                    Vec3::new(
                        if i & 1 == 0 { -h.x } else { h.x },
                        if i & 2 == 0 { -h.y } else { h.y },
                        if i & 4 == 0 { -h.z } else { h.z },
                    )
                });
                Aabb::of_points(corners.map(|c| t.apply(c)))
            }
            Shape::Sphere { radius } => {
                let c = t.apply(Vec3::ZERO);
                let r = radius * t.scale;
                Aabb {
                    min: c - Vec3::new(r, r, r),
                    max: c + Vec3::new(r, r, r),
                }
            }
            Shape::Capsule { a, b, radius } => {
                let r = radius * t.scale;
                Aabb::of_points([t.apply(*a), t.apply(*b)]).grow(r)
            }
            Shape::ConvexHull { points } => Aabb::of_points(points.iter().map(|p| t.apply(*p))),
            Shape::TriangleMesh { vertices, .. } => {
                Aabb::of_points(vertices.iter().map(|p| t.apply(*p)))
            }
            Shape::Compound(children) => children
                .iter()
                .map(|(ct, c)| c.aabb(&t.then(ct)))
                .fold(Aabb::empty(), |a, b| a.union(&b)),
        }
    }

    pub fn triangle_count(&self) -> usize {
        match self {
            Shape::Material { shape, .. } => shape.triangle_count(),
            Shape::TriangleMesh { triangles, .. } => triangles.len(),
            Shape::Compound(children) => children.iter().map(|(_, c)| c.triangle_count()).sum(),
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    /// The scene object that carries the collision.
    pub node: BlockRef,
    pub node_name: String,
    pub layer: Layer,
    pub motion: MotionType,
    /// Kilograms. Bethesda marks immovable geometry with a mass of zero
    /// and leaves the motion type at a dynamic value, so this, not the
    /// motion type, is what says a body is static.
    pub mass: f32,
    /// World transform of the shape, game units.
    pub transform: Iso,
    pub shape: Shape,
}

impl Collider {
    pub fn aabb(&self) -> Aabb {
        self.shape.aabb(&self.transform)
    }

    /// Statics are what a world loads for a placed object. Everything else
    /// is simulated or driven by animation and is the game layer's job.
    /// Vanilla statics (a farmhouse, a chest) carry mass zero with motion
    /// system 5, so mass decides and the motion type is a second signal.
    pub fn is_static(&self) -> bool {
        self.mass <= 0.0 || matches!(self.motion, MotionType::Fixed | MotionType::Keyframed)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Collected {
    pub colliders: Vec<Collider>,
    /// Blocks that were referenced but not decoded, with the reason.
    pub skipped: Vec<(BlockRef, String)>,
}

/// Every collider a file places, in the file's root space.
pub fn collect(file: &NifFile) -> Result<Collected> {
    collect_blocks(&file.header, &file.blocks, &file.roots)
}

/// The same over the raw parts, for callers that build block lists
/// themselves.
pub fn collect_blocks(header: &Header, blocks: &[Block], roots: &[BlockRef]) -> Result<Collected> {
    let mut w = Walker {
        header,
        blocks,
        out: Collected::default(),
        seen: HashSet::new(),
    };
    for &r in roots {
        w.walk(r, &Iso::IDENTITY, 0)?;
    }
    Ok(w.out)
}

struct Walker<'a> {
    header: &'a Header,
    blocks: &'a [Block],
    out: Collected,
    seen: HashSet<usize>,
}

const MAX_DEPTH: usize = 256;

impl<'a> Walker<'a> {
    fn block(&self, r: BlockRef) -> Option<&'a Block> {
        r.get().and_then(|i| self.blocks.get(i))
    }

    fn bhk(&self, r: BlockRef) -> Result<Option<Bhk>> {
        match self.block(r) {
            Some(Block::Collision { type_name, data }) => {
                bhk::decode(type_name, data, self.header).map(Some)
            }
            _ => Ok(None),
        }
    }

    fn walk(&mut self, r: BlockRef, parent: &Iso, depth: usize) -> Result<()> {
        let Some(i) = r.get() else {
            return Ok(());
        };
        if depth > MAX_DEPTH || !self.seen.insert(i) {
            return Ok(());
        }
        let Some(block) = self.blocks.get(i) else {
            return Ok(());
        };
        let (av, children): (&AvObject, &[BlockRef]) = match block {
            Block::Node(n) => (&n.av, &n.children),
            Block::TriShape(t) => (&t.av, &[]),
            Block::BsTriShape(t) => (&t.av, &[]),
            _ => return Ok(()),
        };
        let world = parent.then(&Iso::from(av.transform));
        if !av.collision.is_none() {
            self.collision(r, av.name(), av.collision, &world)?;
        }
        for &c in children {
            self.walk(c, &world, depth + 1)?;
        }
        Ok(())
    }

    fn collision(&mut self, node: BlockRef, name: &str, co: BlockRef, world: &Iso) -> Result<()> {
        let body_ref = match self.bhk(co)? {
            Some(Bhk::CollisionObject { body, .. }) => body,
            Some(Bhk::Other(t)) => {
                self.out.skipped.push((co, format!("collision object {t}")));
                return Ok(());
            }
            Some(_) => {
                self.out
                    .skipped
                    .push((co, "collision reference is not a collision object".into()));
                return Ok(());
            }
            None => return Ok(()),
        };
        let body = match self.bhk(body_ref)? {
            Some(Bhk::RigidBody(b)) => b,
            Some(Bhk::Other(t)) => {
                self.out.skipped.push((body_ref, format!("body {t}")));
                return Ok(());
            }
            _ => {
                self.out
                    .skipped
                    .push((body_ref, "body reference is not a rigid body".into()));
                return Ok(());
            }
        };
        let Some(shape) = self.shape(body.shape, 0)? else {
            return Ok(());
        };
        let local = body.local_transform();
        let local = Iso {
            translation: local.translation * UNITS_PER_METER,
            ..local
        };
        self.out.colliders.push(Collider {
            node,
            node_name: name.to_string(),
            layer: body.layer,
            motion: body.motion,
            mass: body.mass,
            transform: world.then(&local),
            shape: shape.scaled(UNITS_PER_METER),
        });
        Ok(())
    }

    /// The shape tree under `r` in Havok metres, `None` when nothing
    /// usable is there (recorded in `skipped`).
    fn shape(&mut self, r: BlockRef, depth: usize) -> Result<Option<Shape>> {
        if depth > 32 {
            return Err(Error::Invalid("shape tree deeper than 32".into()));
        }
        let Some(b) = self.bhk(r)? else {
            self.out
                .skipped
                .push((r, "shape reference is not a bhk block".into()));
            return Ok(None);
        };
        let material = match &b {
            Bhk::Box { material, .. }
            | Bhk::Sphere { material, .. }
            | Bhk::Capsule { material, .. }
            | Bhk::ConvexVertices { material, .. }
            | Bhk::Transform { material, .. }
            | Bhk::List { material, .. } => Some(*material),
            _ => None,
        };
        let shape = match b {
            Bhk::Box { half_extents, .. } => Some(Shape::Box { half_extents }),
            Bhk::Sphere { radius, .. } => Some(Shape::Sphere { radius }),
            Bhk::Capsule {
                radius,
                first,
                second,
                ..
            } => Some(Shape::Capsule {
                a: first,
                b: second,
                radius,
            }),
            Bhk::ConvexVertices { vertices, .. } => {
                if vertices.len() < 4 {
                    self.out
                        .skipped
                        .push((r, "convex shape with fewer than four points".into()));
                    None
                } else {
                    Some(Shape::ConvexHull { points: vertices })
                }
            }
            Bhk::Transform {
                shape, transform, ..
            } => self
                .shape(shape, depth + 1)?
                .map(|s| Shape::Compound(vec![(transform, s)])),
            Bhk::List { children, .. } => {
                let mut out = Vec::new();
                for c in children {
                    if let Some(s) = self.shape(c, depth + 1)? {
                        out.push((Iso::IDENTITY, s));
                    }
                }
                if out.is_empty() {
                    None
                } else {
                    Some(Shape::Compound(out))
                }
            }
            Bhk::MoppBvTree { shape, scale } => {
                let inner = self.shape(shape, depth + 1)?;
                if (scale - 1.0).abs() > 1e-6 {
                    inner.map(|s| s.scaled(scale))
                } else {
                    inner
                }
            }
            Bhk::CompressedMesh { data, scale, .. } => match self.bhk(data)? {
                Some(Bhk::CompressedMeshData(d)) => {
                    let mesh = compressed_mesh(&d, scale);
                    if mesh.triangle_count() == 0 {
                        self.out
                            .skipped
                            .push((data, "compressed mesh with no triangles".into()));
                        None
                    } else {
                        Some(mesh)
                    }
                }
                _ => {
                    self.out
                        .skipped
                        .push((data, "compressed mesh data missing".into()));
                    None
                }
            },
            Bhk::Other(t) => {
                self.out.skipped.push((r, format!("shape {t}")));
                None
            }
            Bhk::CollisionObject { .. } | Bhk::RigidBody(_) | Bhk::CompressedMeshData(_) => {
                self.out
                    .skipped
                    .push((r, "shape reference points at a non-shape block".into()));
                None
            }
        };
        Ok(shape.map(|shape| match material {
            Some(id) => Shape::Material {
                id,
                shape: Box::new(shape),
            },
            None => shape,
        }))
    }
}

/// Chunks and big triangles as one triangle mesh in the shape's space.
fn compressed_mesh(d: &CompressedMeshData, scale: Vec3) -> Shape {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut materials = Vec::new();
    let s = |v: Vec3| Vec3::new(v.x * scale.x, v.y * scale.y, v.z * scale.z);
    for chunk in &d.chunks {
        let base = vertices.len() as u32;
        let transform = d
            .transforms
            .get(chunk.transform_index as usize)
            .copied()
            .unwrap_or(Iso::IDENTITY);
        vertices.extend(chunk.positions().map(|p| s(transform.apply(p))));
        let n = chunk.vertices.len() as u16;
        for t in chunk.triangles() {
            if t.iter().all(|&i| i < n) {
                triangles.push([base + t[0] as u32, base + t[1] as u32, base + t[2] as u32]);
                materials.push(d.materials.get(chunk.material_index as usize).map(|m| m.0));
            }
        }
    }
    let base = vertices.len() as u32;
    vertices.extend(d.big_vertices.iter().map(|&p| s(p)));
    let n = d.big_vertices.len() as u16;
    for (t, material_index) in &d.big_triangles {
        if t.iter().all(|&i| i < n) {
            triangles.push([base + t[0] as u32, base + t[1] as u32, base + t[2] as u32]);
            materials.push(d.materials.get(*material_index as usize).map(|m| m.0));
        }
    }
    Shape::TriangleMesh {
        vertices,
        triangles,
        materials,
    }
}
