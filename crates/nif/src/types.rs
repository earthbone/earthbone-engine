// SPDX-License-Identifier: GPL-3.0-or-later

//! Small value types shared by the block layer and the mesh layer. All of
//! them keep the file's own conventions: NIF space is right-handed with Z up
//! and one unit is one game unit (roughly 1.4 cm). Converting to a host
//! engine's axes is the host's job.

/// A block index as stored in the file. `-1` means no block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct BlockRef(pub i32);

impl BlockRef {
    pub const NONE: BlockRef = BlockRef(-1);

    pub fn get(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    pub fn is_none(self) -> bool {
        self.0 < 0
    }
}

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

    pub fn new(x: f32, y: f32, z: f32) -> Vec3 {
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

    pub fn scale(self, s: f32) -> Vec3 {
        Vec3 {
            x: self.x * s,
            y: self.y * s,
            z: self.z * s,
        }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;

    fn add(self, o: Vec3) -> Vec3 {
        Vec3 {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
    }
}

/// A 3 by 3 rotation matrix, `rows[r][c]`. The file stores it column by
/// column (m11, m21, m31, m12, ...) and the reader transposes on the way in
/// so that `apply` is the plain row-times-vector product.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat33 {
    pub rows: [[f32; 3]; 3],
}

impl Default for Mat33 {
    fn default() -> Self {
        Mat33::IDENTITY
    }
}

impl Mat33 {
    pub const IDENTITY: Mat33 = Mat33 {
        rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    pub fn apply(&self, v: Vec3) -> Vec3 {
        let r = &self.rows;
        Vec3 {
            x: r[0][0] * v.x + r[0][1] * v.y + r[0][2] * v.z,
            y: r[1][0] * v.x + r[1][1] * v.y + r[1][2] * v.z,
            z: r[2][0] * v.x + r[2][1] * v.y + r[2][2] * v.z,
        }
    }

    pub fn mul(&self, o: &Mat33) -> Mat33 {
        let mut rows = [[0.0f32; 3]; 3];
        for (r, row) in rows.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.rows[r][k] * o.rows[k][c]).sum();
            }
        }
        Mat33 { rows }
    }
}

/// Rotation, translation and uniform scale, in that order of application:
/// `world = rotation * (local * scale) + translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub rotation: Mat33,
    pub translation: Vec3,
    pub scale: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        rotation: Mat33::IDENTITY,
        translation: Vec3::ZERO,
        scale: 1.0,
    };

    pub fn apply(&self, v: Vec3) -> Vec3 {
        self.rotation.apply(v.scale(self.scale)) + self.translation
    }

    /// Rotate a direction: no translation, no scale.
    pub fn apply_direction(&self, v: Vec3) -> Vec3 {
        self.rotation.apply(v)
    }

    /// `self` then `child`, giving the child's transform in `self`'s parent
    /// space. This is how a scene graph accumulates from root to leaf.
    pub fn then(&self, child: &Transform) -> Transform {
        Transform {
            rotation: self.rotation.mul(&child.rotation),
            translation: self.apply(child.translation),
            scale: self.scale * child.scale,
        }
    }
}

/// A bounding sphere.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Bound {
    pub center: Vec3,
    pub radius: f32,
}

/// A unit quaternion, `w` first as the file stores it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}

impl Quat {
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn dot(self, o: Quat) -> f32 {
        self.w * o.w + self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Normalized, falling back to identity when the length is too small to
    /// divide by, which only happens for a malformed key.
    pub fn normalized(self) -> Quat {
        let len = self.length();
        if len < 1e-8 {
            return Quat::IDENTITY;
        }
        Quat {
            w: self.w / len,
            x: self.x / len,
            y: self.y / len,
            z: self.z / len,
        }
    }

    fn negate(self) -> Quat {
        Quat {
            w: -self.w,
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }

    /// The rotation matrix a unit quaternion represents, in the same
    /// row-major, row-times-vector convention as [`Mat33::apply`].
    pub fn to_mat33(self) -> Mat33 {
        let Quat { w, x, y, z } = self.normalized();
        Mat33 {
            rows: [
                [
                    1.0 - 2.0 * (y * y + z * z),
                    2.0 * (x * y - w * z),
                    2.0 * (x * z + w * y),
                ],
                [
                    2.0 * (x * y + w * z),
                    1.0 - 2.0 * (x * x + z * z),
                    2.0 * (y * z - w * x),
                ],
                [
                    2.0 * (x * z - w * y),
                    2.0 * (y * z + w * x),
                    1.0 - 2.0 * (x * x + y * y),
                ],
            ],
        }
    }

    /// Shortest-path spherical interpolation. Falls back to a normalized
    /// linear blend when the two keys are close enough that the great-circle
    /// path is numerically unstable, the usual guard for this formula.
    pub fn slerp(self, mut b: Quat, t: f32) -> Quat {
        let a = self;
        let mut dot = a.dot(b);
        if dot < 0.0 {
            b = b.negate();
            dot = -dot;
        }
        if dot > 0.9995 {
            return Quat {
                w: a.w + (b.w - a.w) * t,
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
                z: a.z + (b.z - a.z) * t,
            }
            .normalized();
        }
        let theta_0 = dot.clamp(-1.0, 1.0).acos();
        let theta = theta_0 * t;
        let sin_theta_0 = theta_0.sin();
        let s0 = (theta_0 - theta).sin() / sin_theta_0;
        let s1 = theta.sin() / sin_theta_0;
        Quat {
            w: a.w * s0 + b.w * s1,
            x: a.x * s0 + b.x * s1,
            y: a.y * s0 + b.y * s1,
            z: a.z * s0 + b.z * s1,
        }
    }
}
