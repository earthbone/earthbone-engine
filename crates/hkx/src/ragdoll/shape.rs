// SPDX-License-Identifier: GPL-3.0-or-later

//! Ragdoll collision shapes using serde-hkx's hkp shape class layouts.

use std::collections::BTreeSet;

use crate::{Error, Obj, Result};

use super::Transform;

/// A convex hull with unpacked xyz vertices and authored plane equations.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvexHull {
    pub radius: f32,
    pub aabb_half_extents: [f32; 4],
    pub aabb_center: [f32; 4],
    pub vertices: Vec<[f32; 3]>,
    pub planes: Vec<[f32; 4]>,
}

/// Typed geometry, in the body's local coordinate system.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Capsule {
        radius: f32,
        a: [f32; 4],
        b: [f32; 4],
    },
    Box {
        radius: f32,
        half_extents: [f32; 4],
    },
    Sphere {
        radius: f32,
    },
    ConvexHull(ConvexHull),
    Translated {
        radius: f32,
        translation: [f32; 4],
        child: Box<Shape>,
    },
    Transformed {
        radius: f32,
        transform: Transform,
        child: Box<Shape>,
    },
}

impl Shape {
    pub(super) fn from_obj(o: Obj<'_>) -> Result<Self> {
        Self::read(o, &mut BTreeSet::new())
    }

    fn read(o: Obj<'_>, path: &mut BTreeSet<u32>) -> Result<Self> {
        if path.len() >= 64 || !path.insert(o.base) {
            return Err(Error::BadRagdoll(
                "cyclic or excessively nested collision shape".into(),
            ));
        }
        let shape = match o.class() {
            Some("hkpCapsuleShape") => Self::Capsule {
                radius: o.f32(32)?,
                a: o.vec4(48)?,
                b: o.vec4(64)?,
            },
            Some("hkpBoxShape") => Self::Box {
                radius: o.f32(32)?,
                half_extents: o.vec4(48)?,
            },
            Some("hkpSphereShape") => Self::Sphere { radius: o.f32(32)? },
            Some("hkpConvexVerticesShape") => Self::ConvexHull(ConvexHull::from_obj(o)?),
            Some("hkpConvexTranslateShape") => Self::Translated {
                radius: o.f32(32)?,
                translation: o.vec4(64)?,
                child: Box::new(Self::read(
                    o.ptr_required(48, "translated shape child")?,
                    path,
                )?),
            },
            Some("hkpConvexTransformShape") => Self::Transformed {
                radius: o.f32(32)?,
                transform: Transform::from_obj(o.at(64))?,
                child: Box::new(Self::read(
                    o.ptr_required(48, "transformed shape child")?,
                    path,
                )?),
            },
            class => {
                return Err(Error::Unsupported(format!(
                    "ragdoll shape {}",
                    class.unwrap_or("<unnamed>")
                )))
            }
        };
        path.remove(&o.base);
        Ok(shape)
    }
}

impl ConvexHull {
    fn from_obj(o: Obj<'_>) -> Result<Self> {
        let count = usize::try_from(o.i32(96)?)
            .map_err(|_| Error::BadRagdoll("negative convex vertex count".into()))?;
        let groups = o.array_structs(80, 48)?;
        if count > groups.len().saturating_mul(4) {
            return Err(Error::BadRagdoll(
                "convex vertex count exceeds stored vectors".into(),
            ));
        }
        let mut vertices = Vec::with_capacity(count);
        for group in groups {
            let x = group.vec4(0)?;
            let y = group.vec4(16)?;
            let z = group.vec4(32)?;
            for i in 0..4 {
                if vertices.len() < count {
                    vertices.push([x[i], y[i], z[i]]);
                }
            }
        }
        Ok(Self {
            radius: o.f32(32)?,
            aabb_half_extents: o.vec4(48)?,
            aabb_center: o.vec4(64)?,
            vertices,
            planes: o.array_vec4(120)?,
        })
    }
}
