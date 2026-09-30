// SPDX-License-Identifier: GPL-3.0-or-later

//! The small amount of vector algebra the shape conversion and the fallback
//! world need. Everything is `f32`, right-handed, Z up, in game units once
//! it leaves the `bhk` module. `soe_nif` has its own `Vec3` for file data,
//! `From` conversions bridge the two.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Z: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3 {
            x: self.y * o.z - self.z * o.y,
            y: self.z * o.x - self.x * o.z,
            z: self.x * o.y - self.y * o.x,
        }
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Unit vector, or zero when the input is too short to have a direction.
    pub fn normalized(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 {
            self * (1.0 / l)
        } else {
            Vec3::ZERO
        }
    }

    fn componentwise(self, o: Vec3, op: fn(f32, f32) -> f32) -> Vec3 {
        Vec3::new(op(self.x, o.x), op(self.y, o.y), op(self.z, o.z))
    }

    pub fn min(self, o: Vec3) -> Vec3 {
        self.componentwise(o, f32::min)
    }

    pub fn max(self, o: Vec3) -> Vec3 {
        self.componentwise(o, f32::max)
    }

    pub fn abs(self) -> Vec3 {
        Vec3::new(self.x.abs(), self.y.abs(), self.z.abs())
    }

    pub fn max_element(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl From<soe_nif::Vec3> for Vec3 {
    fn from(v: soe_nif::Vec3) -> Vec3 {
        Vec3::new(v.x, v.y, v.z)
    }
}

impl From<Vec3> for soe_nif::Vec3 {
    fn from(v: Vec3) -> soe_nif::Vec3 {
        soe_nif::Vec3::new(v.x, v.y, v.z)
    }
}

/// A unit quaternion, `x, y, z, w` as Havok stores it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Quat {
        Quat { x, y, z, w }
    }

    /// Rotation of `angle` radians about a unit `axis`.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let (s, c) = (angle * 0.5).sin_cos();
        let a = axis.normalized() * s;
        Quat::new(a.x, a.y, a.z, c)
    }

    pub fn norm(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt()
    }

    pub fn to_mat3(self) -> Mat3 {
        let Quat { x, y, z, w } = self;
        Mat3 {
            rows: [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y - z * w),
                    2.0 * (x * z + y * w),
                ],
                [
                    2.0 * (x * y + z * w),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z - x * w),
                ],
                [
                    2.0 * (x * z - y * w),
                    2.0 * (y * z + x * w),
                    1.0 - 2.0 * (x * x + y * y),
                ],
            ],
        }
    }
}

/// A 3 by 3 matrix, `rows[r][c]`, applied as row times column vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub rows: [[f32; 3]; 3],
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    pub fn apply(&self, v: Vec3) -> Vec3 {
        let r = &self.rows;
        Vec3::new(
            r[0][0] * v.x + r[0][1] * v.y + r[0][2] * v.z,
            r[1][0] * v.x + r[1][1] * v.y + r[1][2] * v.z,
            r[2][0] * v.x + r[2][1] * v.y + r[2][2] * v.z,
        )
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let mut rows = [[0.0; 3]; 3];
        for (r, row) in rows.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.rows[r][k] * o.rows[k][c]).sum();
            }
        }
        Mat3 { rows }
    }

    pub fn transpose(&self) -> Mat3 {
        let r = &self.rows;
        Mat3 {
            rows: [
                [r[0][0], r[1][0], r[2][0]],
                [r[0][1], r[1][1], r[2][1]],
                [r[0][2], r[1][2], r[2][2]],
            ],
        }
    }

    /// Largest scale the matrix applies along any axis, for bounding a
    /// rotated and scaled shape.
    pub fn max_scale(&self) -> f32 {
        self.rows
            .iter()
            .map(|r| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt())
            .fold(0.0, f32::max)
    }
}

impl From<soe_nif::Mat33> for Mat3 {
    fn from(m: soe_nif::Mat33) -> Mat3 {
        Mat3 { rows: m.rows }
    }
}

/// A similarity transform: rotate and uniformly scale, then translate. The
/// NIF scene graph carries a scale per node and so does this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iso {
    pub rotation: Mat3,
    pub translation: Vec3,
    pub scale: f32,
}

impl Iso {
    pub const IDENTITY: Iso = Iso {
        rotation: Mat3::IDENTITY,
        translation: Vec3::ZERO,
        scale: 1.0,
    };

    pub fn from_parts(rotation: Mat3, translation: Vec3) -> Iso {
        Iso {
            rotation,
            translation,
            scale: 1.0,
        }
    }

    pub fn translation(t: Vec3) -> Iso {
        Iso::from_parts(Mat3::IDENTITY, t)
    }

    /// The placement a Bethesda reference record describes: a position, an
    /// X, Y, Z Euler rotation in radians, and a uniform scale, exactly as
    /// `REFR`'s `DATA` and `XSCL` carry them.
    ///
    /// The matrix order is the Creation Engine's, so a reference placed in
    /// the editor lands where the editor showed it. It lives here because
    /// both the renderer and the collision builder need the same answer, and
    /// two copies of a rotation convention drift.
    ///
    /// A zero scale is read as one, which is what the engine does with a
    /// record that omits `XSCL`.
    #[must_use]
    pub fn placement(position: [f32; 3], rotation_xyz: [f32; 3], scale: f32) -> Iso {
        let [rx, ry, rz] = rotation_xyz;
        let (sx, cx) = rx.sin_cos();
        let (sy, cy) = ry.sin_cos();
        let (sz, cz) = rz.sin_cos();
        Iso {
            rotation: Mat3 {
                rows: [
                    [cy * cz, cy * sz, -sy],
                    [sx * sy * cz - cx * sz, sx * sy * sz + cx * cz, sx * cy],
                    [cx * sy * cz + sx * sz, cx * sy * sz - sx * cz, cx * cy],
                ],
            },
            translation: Vec3::new(position[0], position[1], position[2]),
            scale: if scale == 0.0 { 1.0 } else { scale },
        }
    }

    pub fn apply(&self, v: Vec3) -> Vec3 {
        self.rotation.apply(v * self.scale) + self.translation
    }

    pub fn apply_direction(&self, v: Vec3) -> Vec3 {
        self.rotation.apply(v)
    }

    /// `self` then `child`: the child's transform expressed in `self`'s
    /// parent space.
    pub fn then(&self, child: &Iso) -> Iso {
        Iso {
            rotation: self.rotation.mul(&child.rotation),
            translation: self.apply(child.translation),
            scale: self.scale * child.scale,
        }
    }

    /// Inverse, valid while the rotation is orthonormal and the scale is
    /// not zero.
    pub fn inverse(&self) -> Iso {
        let rotation = self.rotation.transpose();
        let scale = 1.0 / self.scale;
        let translation = rotation.apply(-self.translation) * scale;
        Iso {
            rotation,
            translation,
            scale,
        }
    }
}

impl From<soe_nif::Transform> for Iso {
    fn from(t: soe_nif::Transform) -> Iso {
        Iso {
            rotation: t.rotation.into(),
            translation: t.translation.into(),
            scale: t.scale,
        }
    }
}

/// Axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn empty() -> Aabb {
        Aabb {
            min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    pub fn of_points<I: IntoIterator<Item = Vec3>>(points: I) -> Aabb {
        let mut b = Aabb::empty();
        for p in points {
            b.min = b.min.min(p);
            b.max = b.max.max(p);
        }
        b
    }

    pub fn grow(&self, by: f32) -> Aabb {
        let d = Vec3::new(by, by, by);
        Aabb {
            min: self.min - d,
            max: self.max + d,
        }
    }

    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb {
            min: self.min.min(o.min),
            max: self.max.max(o.max),
        }
    }

    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x <= o.max.x
            && self.max.x >= o.min.x
            && self.min.y <= o.max.y
            && self.max.y >= o.min.y
            && self.min.z <= o.max.z
            && self.max.z >= o.min.z
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_about_z_rotates_x_to_y() {
        let q = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2);
        let v = q.to_mat3().apply(Vec3::new(1.0, 0.0, 0.0));
        assert!((v - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn iso_inverse_round_trips() {
        let r = Quat::from_axis_angle(Vec3::new(1.0, 2.0, 3.0), 0.7).to_mat3();
        let t = Iso {
            rotation: r,
            translation: Vec3::new(10.0, -4.0, 2.5),
            scale: 2.0,
        };
        let p = Vec3::new(3.0, 1.0, -7.0);
        let back = t.inverse().apply(t.apply(p));
        assert!((back - p).length() < 1e-4);
    }

    #[test]
    fn then_composes_parent_before_child() {
        let parent = Iso::translation(Vec3::new(1.0, 0.0, 0.0));
        let child = Iso {
            rotation: Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2).to_mat3(),
            translation: Vec3::new(0.0, 1.0, 0.0),
            scale: 1.0,
        };
        let world = parent.then(&child);
        let p = world.apply(Vec3::new(1.0, 0.0, 0.0));
        assert!((p - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-5);
    }
}
