// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning the interpreter's output into something a renderer can skin with.
//!
//! [`crate::interp::Interpreter`] gives a pose as one **local** transform per
//! bone, the same shape as [`crate::skeleton::Skeleton::reference_pose`]. A
//! renderer needs each bone in **model** space, which means composing every
//! bone with its parent chain. That is all this module does, plus the
//! quaternion arithmetic the composition needs, which `hkQsTransform` carries
//! in the Havok SDK and which nothing else in this crate had needed yet.
//!
//! Havok skeletons store parents before children, so one forward pass is
//! enough and no recursion or visited set is required. [`to_model_space`]
//! checks that ordering rather than assuming it, because a file that broke it
//! would otherwise produce a silently wrong pose.

use crate::QsTransform;

/// Hamilton product, `a` then `b` applied to a vector as `a * b * v`.
#[must_use]
pub fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// Rotate a vector by a unit quaternion.
#[must_use]
pub fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let u = [q[0], q[1], q[2]];
    let w = q[3];
    let uv = cross(u, v);
    let uuv = cross(u, uv);
    [
        v[0] + 2.0 * (w * uv[0] + uuv[0]),
        v[1] + 2.0 * (w * uv[1] + uuv[1]),
        v[2] + 2.0 * (w * uv[2] + uuv[2]),
    ]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `parent` then `child`, giving the child in the parent's own space.
///
/// Matches `hkQsTransform::setMul`: the parent's scale applies to the child's
/// translation before the parent's rotation carries it, and scales multiply
/// per axis.
#[must_use]
pub fn compose(parent: &QsTransform, child: &QsTransform) -> QsTransform {
    let scaled = [
        child.translation[0] * parent.scale[0],
        child.translation[1] * parent.scale[1],
        child.translation[2] * parent.scale[2],
    ];
    let rotated = quat_rotate(parent.rotation, scaled);
    QsTransform {
        translation: [
            parent.translation[0] + rotated[0],
            parent.translation[1] + rotated[1],
            parent.translation[2] + rotated[2],
            0.0,
        ],
        rotation: quat_mul(parent.rotation, child.rotation),
        scale: [
            parent.scale[0] * child.scale[0],
            parent.scale[1] * child.scale[1],
            parent.scale[2] * child.scale[2],
            0.0,
        ],
    }
}

/// Why a pose could not be lifted into model space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoseError {
    /// The pose and the parent table disagree about how many bones exist.
    BoneCount { pose: usize, parents: usize },
    /// A bone's parent sits at or after it in the array, so one forward pass
    /// would read a transform that has not been composed yet.
    ParentNotBefore { bone: usize, parent: i16 },
    /// A parent index points outside the skeleton.
    ParentOutOfRange { bone: usize, parent: i16 },
}

impl std::fmt::Display for PoseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoseError::BoneCount { pose, parents } => {
                write!(f, "pose has {pose} bones, the skeleton has {parents}")
            }
            PoseError::ParentNotBefore { bone, parent } => {
                write!(f, "bone {bone} has parent {parent}, which is not before it")
            }
            PoseError::ParentOutOfRange { bone, parent } => {
                write!(f, "bone {bone} has parent {parent}, outside the skeleton")
            }
        }
    }
}

impl std::error::Error for PoseError {}

/// Compose every local transform with its parent chain, in place of the
/// caller allocating.
///
/// `parents` is [`crate::skeleton::Skeleton::parent_indices`], negative for a
/// root. `out` is resized to match.
///
/// # Errors
///
/// [`PoseError`] when the two arrays disagree on length, or when a parent
/// index is out of range or does not come before its child.
pub fn to_model_space_into(
    local: &[QsTransform],
    parents: &[i16],
    out: &mut Vec<QsTransform>,
) -> Result<(), PoseError> {
    if local.len() != parents.len() {
        return Err(PoseError::BoneCount {
            pose: local.len(),
            parents: parents.len(),
        });
    }
    out.clear();
    out.reserve(local.len());
    for (bone, (transform, &parent)) in local.iter().zip(parents).enumerate() {
        if parent < 0 {
            out.push(*transform);
            continue;
        }
        let at = parent as usize;
        if at >= local.len() {
            return Err(PoseError::ParentOutOfRange { bone, parent });
        }
        if at >= bone {
            return Err(PoseError::ParentNotBefore { bone, parent });
        }
        let composed = compose(&out[at], transform);
        out.push(composed);
    }
    Ok(())
}

/// [`to_model_space_into`] with the vector allocated for you.
///
/// # Errors
///
/// As [`to_model_space_into`].
pub fn to_model_space(
    local: &[QsTransform],
    parents: &[i16],
) -> Result<Vec<QsTransform>, PoseError> {
    let mut out = Vec::new();
    to_model_space_into(local, parents, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translated(x: f32, y: f32, z: f32) -> QsTransform {
        QsTransform {
            translation: [x, y, z, 0.0],
            ..QsTransform::IDENTITY
        }
    }

    /// A quarter turn about Z, so x becomes y.
    fn quarter_turn_z() -> QsTransform {
        let h = std::f32::consts::FRAC_PI_4;
        QsTransform {
            rotation: [0.0, 0.0, h.sin(), h.cos()],
            ..QsTransform::IDENTITY
        }
    }

    #[test]
    fn composing_with_identity_changes_nothing() {
        let t = translated(1.0, 2.0, 3.0);
        assert_eq!(compose(&QsTransform::IDENTITY, &t), t);
        let back = compose(&t, &QsTransform::IDENTITY);
        assert_eq!(back.translation, t.translation);
    }

    #[test]
    fn a_parent_rotation_carries_the_child_offset() {
        // Parent turned a quarter about Z, child one unit along X.
        let out = compose(&quarter_turn_z(), &translated(1.0, 0.0, 0.0));
        assert!(out.translation[0].abs() < 1e-6, "{:?}", out.translation);
        assert!(
            (out.translation[1] - 1.0).abs() < 1e-6,
            "{:?}",
            out.translation
        );
    }

    #[test]
    fn a_parent_scale_multiplies_the_child_offset() {
        let parent = QsTransform {
            scale: [2.0, 2.0, 2.0, 0.0],
            ..QsTransform::IDENTITY
        };
        let out = compose(&parent, &translated(3.0, 0.0, 0.0));
        assert!((out.translation[0] - 6.0).abs() < 1e-6);
        assert!((out.scale[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn a_chain_accumulates_down_the_hierarchy() {
        // Three bones in a line, each one unit along X from its parent.
        let local = vec![
            translated(1.0, 0.0, 0.0),
            translated(1.0, 0.0, 0.0),
            translated(1.0, 0.0, 0.0),
        ];
        let model = to_model_space(&local, &[-1, 0, 1]).unwrap();
        assert!((model[0].translation[0] - 1.0).abs() < 1e-6);
        assert!((model[1].translation[0] - 2.0).abs() < 1e-6);
        assert!((model[2].translation[0] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn roots_pass_through_untouched() {
        let local = vec![translated(5.0, 0.0, 0.0), translated(7.0, 0.0, 0.0)];
        let model = to_model_space(&local, &[-1, -1]).unwrap();
        assert_eq!(model[0].translation, local[0].translation);
        assert_eq!(model[1].translation, local[1].translation);
    }

    #[test]
    fn a_length_mismatch_is_refused() {
        let err = to_model_space(&[QsTransform::IDENTITY], &[-1, 0]).unwrap_err();
        assert_eq!(
            err,
            PoseError::BoneCount {
                pose: 1,
                parents: 2
            }
        );
    }

    #[test]
    fn a_parent_after_its_child_is_refused_rather_than_read_early() {
        let local = vec![QsTransform::IDENTITY; 2];
        let err = to_model_space(&local, &[1, -1]).unwrap_err();
        assert_eq!(err, PoseError::ParentNotBefore { bone: 0, parent: 1 });
    }

    #[test]
    fn a_parent_outside_the_skeleton_is_refused() {
        let local = vec![QsTransform::IDENTITY; 2];
        let err = to_model_space(&local, &[-1, 9]).unwrap_err();
        assert_eq!(err, PoseError::ParentOutOfRange { bone: 1, parent: 9 });
    }

    #[test]
    fn rotating_a_vector_matches_the_quaternion_product() {
        let q = quarter_turn_z().rotation;
        let v = quat_rotate(q, [1.0, 0.0, 0.0]);
        assert!(v[0].abs() < 1e-6, "{v:?}");
        assert!((v[1] - 1.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn two_quarter_turns_make_a_half_turn() {
        let q = quarter_turn_z().rotation;
        let half = quat_mul(q, q);
        let v = quat_rotate(half, [1.0, 0.0, 0.0]);
        assert!((v[0] + 1.0).abs() < 1e-5, "{v:?}");
        assert!(v[1].abs() < 1e-5, "{v:?}");
    }
}
