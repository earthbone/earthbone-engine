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
    /// Authored character-controller phantoms. These
    /// are never inserted into the static collision world automatically.
    pub phantoms: Vec<Phantom>,
    /// Blocks that were referenced but not decoded, with the reason.
    pub skipped: Vec<(BlockRef, String)>,
}

impl Collected {
    /// Whether the skeleton carries at least one authored `CHAR_CONTROLLER`
    /// phantom, the same test `crates/physics/tests/controller_geometry.rs`
    /// (CMB-29) uses to count a skeleton as having controller geometry.
    ///
    /// This does not separate a full body hit capsule from a small pushing
    /// bumper. No corpus source under `engine-refs/` names a numeric
    /// material ID for the bumper (`docs/physics.md` records the search:
    /// `MaterialIDs.h`, `nifxml/nif.xml` and `nifly` were checked), so a
    /// skeleton that carries only a bumper still counts as authored here.
    /// [`controller_shape`] resolves the same question when picking which
    /// authored phantom to use.
    pub fn has_authored_controller(&self) -> bool {
        !self.phantoms.is_empty()
    }
}

/// Samples the convex shell at support vertices. Axis directions preserve the
/// exact Havok surface offset on bounds, while diagonal directions retain its
/// rounded corners without multiplying large vertex sets.
fn convex_shell_points(vertices: &[Vec3], radius: f32) -> Vec<Vec3> {
    if radius == 0.0 {
        return vertices.to_vec();
    }

    let mut directions = Vec::with_capacity(26);
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let direction = Vec3::new(x as f32, y as f32, z as f32);
                if direction != Vec3::ZERO {
                    directions.push(direction.normalized());
                }
            }
        }
    }

    let mut points = vertices.to_vec();
    points.reserve(directions.len());
    for direction in directions {
        let support = vertices
            .iter()
            .copied()
            .max_by(|a, b| a.dot(direction).total_cmp(&b.dot(direction)))
            .unwrap_or(Vec3::ZERO);
        points.push(support + direction * radius);
    }
    points
}

/// Sample the two circular ends of a cylinder as a convex point set. The
/// radial samples keep the authored radius, while the axis endpoints retain
/// the cylinder's exact height in the convex hull.
fn cylinder_points(first: Vec3, second: Vec3, radius: f32) -> Vec<Vec3> {
    const SIDES: usize = 32;
    let axis = (second - first).normalized();
    let reference = if axis.z.abs() < 0.9 {
        Vec3::new(0.0, 0.0, 1.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = axis.cross(reference).normalized();
    let v = axis.cross(u).normalized();
    let mut points = Vec::with_capacity(SIDES * 2);
    for endpoint in [first, second] {
        for side in 0..SIDES {
            let angle = std::f32::consts::TAU * side as f32 / SIDES as f32;
            let radial = u * angle.cos() + v * angle.sin();
            points.push(endpoint + radial * radius);
        }
    }
    points
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phantom {
    pub node: BlockRef,
    pub node_name: String,
    pub layer: Layer,
    pub filter_flags: u8,
    pub group: u16,
    /// Root-space placement, game units.
    pub transform: Iso,
    /// Local geometry, game units. Capsule endpoints are centres of the caps.
    pub shape: Shape,
}

/// Template capsule height, Havok metres, before an actor's scale.
///
/// Source: `docs/physics.md`, "HKX character metadata survey". Every
/// installed `hkbCharacterData` object under `characters/*.hkx` (353
/// checked) carries the same `characterControllerInfo.capsuleHeight`, the
/// same value `crates/godot-host/examples/character_metadata_survey.rs`
/// reads at that struct's offset 16. Bethesda ships one template capsule
/// and relies on the actor's runtime scale to size it, the pattern
/// `DynamicCollisionAdjustment` follows too:
/// `AdjustmentHandler::AdjustScale` clones an existing wrapper shape scaled
/// by `actor->GetScale()` rather than deriving new dimensions
/// (github.com/ersh1/DynamicCollisionAdjustment, `src/AdjustmentHandler.cpp`).
const TEMPLATE_CAPSULE_HEIGHT: f32 = 1.7;

/// Template capsule radius, Havok metres, before an actor's scale. Same
/// source as [`TEMPLATE_CAPSULE_HEIGHT`], `characterControllerInfo.capsuleRadius`.
const TEMPLATE_CAPSULE_RADIUS: f32 = 0.4;

/// Largest `scale` for which [`generated_controller_shape`]'s capsule
/// coordinates stay finite in `f32`, all the way through the final
/// `.scaled(UNITS_PER_METER)`. `top` (the dominant coordinate, since
/// `TEMPLATE_CAPSULE_HEIGHT` exceeds `2 * TEMPLATE_CAPSULE_RADIUS`) is
/// `(TEMPLATE_CAPSULE_HEIGHT - TEMPLATE_CAPSULE_RADIUS) * scale`, then
/// multiplied by `UNITS_PER_METER`. Dividing `f32::MAX` by that whole
/// per-scale factor, then again by a safety margin of 8 to cover the
/// intermediate `height` and `radius` products computed before the final
/// scaling, bounds every product in the function well under `f32::MAX`.
const MAX_GENERATED_SCALE: f32 = f32::MAX / (TEMPLATE_CAPSULE_HEIGHT * UNITS_PER_METER * 8.0);

/// A runtime-generated character controller capsule for an actor whose
/// skeleton carries no usable authored `CHAR_CONTROLLER` phantom. `scale`
/// is the actor's overall linear scale, `race.height[sex] * npc.height` in
/// CommonLibSSE-NG's `TESNPC::GetHeight()` (`src/RE/T/TESNPC.cpp`), which
/// `crates/game` resolves from the winning RACE and NPC_ records.
///
/// The capsule stands on the local Z axis with the lower cap's centre at
/// `scale * TEMPLATE_CAPSULE_RADIUS` above the local origin, matching the
/// vanilla convention that an actor's root node sits at its feet.
/// Endpoints are the centres of the hemispherical caps, in game units, the
/// same convention [`Phantom::shape`] documents for authored capsules. A
/// non-finite or non-positive `scale` falls back to 1.0 rather than
/// producing degenerate geometry. A finite, positive `scale` too large for
/// the result to stay finite through `UNITS_PER_METER` is clamped to
/// [`MAX_GENERATED_SCALE`] for the same reason: this function returns
/// finite geometry for any finite, positive input, never `inf` or `NaN`
/// coordinates, since a corrupted or crafted RACE or NPC_ height field only
/// ever passes a finiteness check (`crates/game/src/race.rs`'s
/// `Attributes::from_record`, `crates/game/src/npc.rs`'s `f32_at`), never a
/// magnitude one.
pub fn generated_controller_shape(scale: f32) -> Shape {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale.min(MAX_GENERATED_SCALE)
    } else {
        1.0
    };
    let height = TEMPLATE_CAPSULE_HEIGHT * scale;
    let radius = TEMPLATE_CAPSULE_RADIUS * scale;
    let top = height.max(radius * 2.0) - radius;
    Shape::Capsule {
        a: Vec3::new(0.0, 0.0, radius),
        b: Vec3::new(0.0, 0.0, top),
        radius,
    }
    .scaled(UNITS_PER_METER)
}

/// Local placement and geometry for an actor's controller capsule: the
/// largest authored `CHAR_CONTROLLER` phantom's transform and shape when
/// `collected` has one, or identity and [`generated_controller_shape`]
/// otherwise.
///
/// "Largest" is an extent heuristic (the phantom whose local bounding box
/// has the greatest diagonal), not a material lookup: see
/// [`Collected::has_authored_controller`] for why no material ID
/// distinguishes a bumper from a main hit shape here. A skeleton with a
/// single phantom, authored or generated, is unaffected by the heuristic.
pub fn controller_shape(collected: &Collected, scale: f32) -> (Iso, Shape) {
    let extent = |shape: &Shape| {
        let aabb = shape.aabb(&Iso::IDENTITY);
        (aabb.max - aabb.min).length()
    };
    let authored = collected
        .phantoms
        .iter()
        .max_by(|a, b| extent(&a.shape).total_cmp(&extent(&b.shape)));
    match authored {
        Some(phantom) => (phantom.transform, phantom.shape.clone()),
        None => (Iso::IDENTITY, generated_controller_shape(scale)),
    }
}

fn finite_point(v: Vec3) -> bool {
    v.x.is_finite() && v.y.is_finite() && v.z.is_finite()
}

fn valid_transform(t: &Iso) -> bool {
    let m = t.rotation.rows;
    let determinant = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    finite_point(t.translation)
        && t.scale.is_finite()
        && t.scale > 0.0
        && m.iter().flatten().all(|v| v.is_finite())
        && determinant.is_finite()
        && determinant != 0.0
}

fn valid_phantom_shape(shape: &Shape) -> bool {
    match shape {
        Shape::Box { half_extents: v } => finite_point(*v) && v.x > 0.0 && v.y > 0.0 && v.z > 0.0,
        Shape::Sphere { radius } => radius.is_finite() && *radius > 0.0,
        Shape::Capsule { a, b, radius } => {
            finite_point(*a) && finite_point(*b) && radius.is_finite() && *radius > 0.0
        }
        Shape::ConvexHull { points } => {
            points.len() >= 4 && points.iter().copied().all(finite_point)
        }
        Shape::TriangleMesh {
            vertices,
            triangles,
            ..
        } => {
            !triangles.is_empty()
                && vertices.iter().copied().all(finite_point)
                && triangles
                    .iter()
                    .flatten()
                    .all(|i| (*i as usize) < vertices.len())
        }
        Shape::Material { shape, .. } => valid_phantom_shape(shape),
        Shape::Compound(children) => {
            !children.is_empty()
                && children
                    .iter()
                    .all(|(t, s)| valid_transform(t) && valid_phantom_shape(s))
        }
    }
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
            Some(Bhk::SimpleShapePhantom(body)) => {
                if body.layer != Layer::CHAR_CONTROLLER {
                    self.out
                        .skipped
                        .push((body_ref, "non-controller phantom".into()));
                    return Ok(());
                }
                let previous_skips = self.out.skipped.len();
                if let Some(shape) = self.shape(body.shape, 0)? {
                    let local = Iso {
                        translation: body.transform.translation * UNITS_PER_METER,
                        ..body.transform
                    };
                    let phantom = Phantom {
                        node,
                        node_name: name.to_string(),
                        layer: body.layer,
                        filter_flags: body.filter_flags,
                        group: body.group,
                        transform: world.then(&local),
                        shape: shape.scaled(UNITS_PER_METER),
                    };
                    if previous_skips != self.out.skipped.len() {
                        return Err(Error::Unsupported("incomplete phantom shape tree".into()));
                    }
                    if !valid_transform(&phantom.transform) || !valid_phantom_shape(&phantom.shape)
                    {
                        return Err(Error::Invalid("phantom geometry or transform".into()));
                    }
                    self.out.phantoms.push(phantom);
                } else {
                    return Err(Error::Unsupported("missing phantom shape geometry".into()));
                }
                return Ok(());
            }
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
            | Bhk::Cylinder { material, .. }
            | Bhk::ConvexVertices { material, .. }
            | Bhk::Transform { material, .. }
            | Bhk::List { material, .. }
            | Bhk::NiTriStrips { material, .. } => Some(*material),
            _ => None,
        };
        let shape = match b {
            Bhk::Box {
                radius,
                half_extents,
                ..
            } => Some(Shape::Box {
                half_extents: half_extents + Vec3::new(radius, radius, radius),
            }),
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
            Bhk::Cylinder {
                shell_radius,
                radius,
                first,
                second,
                ..
            } => Some(Shape::ConvexHull {
                points: cylinder_points(first, second, radius + shell_radius),
            }),
            Bhk::ConvexVertices {
                radius, vertices, ..
            } => {
                if vertices.len() < 4 {
                    self.out
                        .skipped
                        .push((r, "convex shape with fewer than four points".into()));
                    None
                } else {
                    Some(Shape::ConvexHull {
                        points: convex_shell_points(&vertices, radius),
                    })
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
            Bhk::NiTriStrips { strips, scale, .. } => {
                let mut vertices = Vec::new();
                let mut triangles = Vec::new();
                let mut materials = Vec::new();
                for strip_ref in strips {
                    match self.block(strip_ref) {
                        Some(Block::GeometryData(data)) if data.from_strips => {
                            let base = u32::try_from(vertices.len()).map_err(|_| {
                                Error::Invalid("triangle strip mesh has too many vertices".into())
                            })?;
                            let local_vertex_count = data.vertices.len();
                            vertices.extend(
                                data.vertices.iter().map(|v| {
                                    Vec3::new(v.x * scale.x, v.y * scale.y, v.z * scale.z)
                                }),
                            );
                            for (index, triangle) in data.triangles.iter().enumerate() {
                                if triangle
                                    .iter()
                                    .any(|vertex| usize::from(*vertex) >= local_vertex_count)
                                {
                                    return Err(Error::Invalid(format!(
                                        "triangle strip block {} triangle {index} indexes past its vertices",
                                        strip_ref.get().unwrap_or_default()
                                    )));
                                }
                                triangles.push([
                                    base + u32::from(triangle[0]),
                                    base + u32::from(triangle[1]),
                                    base + u32::from(triangle[2]),
                                ]);
                                materials.push(material);
                            }
                        }
                        Some(Block::GeometryData(_)) => self.out.skipped.push((
                            strip_ref,
                            "triangle strip reference is not NiTriStripsData".into(),
                        )),
                        Some(_) | None => self.out.skipped.push((
                            strip_ref,
                            "triangle strip reference is not geometry data".into(),
                        )),
                    }
                }
                if triangles.is_empty() {
                    None
                } else {
                    Some(Shape::TriangleMesh {
                        vertices,
                        triangles,
                        materials,
                    })
                }
            }
            Bhk::Other(t) => {
                self.out.skipped.push((r, format!("shape {t}")));
                None
            }
            Bhk::CollisionObject { .. }
            | Bhk::RigidBody(_)
            | Bhk::SimpleShapePhantom(_)
            | Bhk::CompressedMeshData(_) => {
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
