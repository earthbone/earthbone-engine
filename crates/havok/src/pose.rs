// SPDX-License-Identifier: GPL-3.0-or-later

//! The allocation-free half of the model-space walk.
//!
//! [`crate::skeleton::Skeleton::to_model_space`] already composes a local pose
//! with its parent chain, and [`crate::skeleton::compose`] is the transform
//! arithmetic under it. Neither is duplicated here. What this module adds is
//! the same walk writing into a buffer the caller keeps, because a renderer
//! runs it once per character per frame and has no reason to allocate a vector
//! of ninety nine transforms sixty times a second.
//!
//! Use [`crate::skeleton::Skeleton::to_model_space`] for a one-off. Use
//! [`to_model_space_into`] in a frame loop.

use crate::skeleton::{compose, Skeleton};
use crate::{Error, QsTransform, Result};

/// Compose every local transform with its parent chain, writing into `out`.
///
/// `out` is cleared and refilled, so its capacity survives between frames.
/// Bones are stored parent first in Havok skeletons, and this checks that
/// rather than assuming it, because a file that broke the ordering would
/// otherwise produce a silently wrong pose.
///
/// # Errors
///
/// [`Error::BadSpline`] when the pose and the skeleton disagree on bone count,
/// or when a bone's parent does not come before it.
pub fn to_model_space_into(
    skeleton: &Skeleton,
    local: &[QsTransform],
    out: &mut Vec<QsTransform>,
) -> Result<()> {
    if local.len() != skeleton.bones.len() {
        return Err(Error::BadSpline(format!(
            "pose has {} transforms for {} bones",
            local.len(),
            skeleton.bones.len()
        )));
    }
    out.clear();
    out.reserve(local.len());
    for (i, t) in local.iter().enumerate() {
        let p = skeleton.parent_indices[i];
        if p < 0 {
            out.push(*t);
        } else if (p as usize) < i {
            let composed = compose(&out[p as usize], t);
            out.push(composed);
        } else {
            return Err(Error::BadSpline(format!(
                "bone {i} has parent {p} which comes later"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::Bone;

    fn chain(len: usize) -> Skeleton {
        Skeleton {
            name: "chain".into(),
            parent_indices: (0..len)
                .map(|i| if i == 0 { -1 } else { (i - 1) as i16 })
                .collect(),
            bones: (0..len)
                .map(|i| Bone {
                    name: format!("b{i}"),
                    lock_translation: false,
                })
                .collect(),
            reference_pose: vec![QsTransform::IDENTITY; len],
            reference_floats: Vec::new(),
            float_slots: Vec::new(),
        }
    }

    fn translated(x: f32) -> QsTransform {
        QsTransform {
            translation: [x, 0.0, 0.0, 0.0],
            ..QsTransform::IDENTITY
        }
    }

    #[test]
    fn it_agrees_with_the_allocating_walk() {
        let skeleton = chain(4);
        let local: Vec<_> = (0..4).map(|i| translated(i as f32 + 1.0)).collect();
        let want = skeleton.to_model_space(&local).unwrap();
        let mut got = Vec::new();
        to_model_space_into(&skeleton, &local, &mut got).unwrap();
        assert_eq!(got, want);
    }

    #[test]
    fn the_buffer_is_reused_rather_than_grown_each_call() {
        let skeleton = chain(8);
        let local = vec![translated(1.0); 8];
        let mut buf = Vec::new();
        to_model_space_into(&skeleton, &local, &mut buf).unwrap();
        let capacity = buf.capacity();
        let ptr = buf.as_ptr();
        for _ in 0..10 {
            to_model_space_into(&skeleton, &local, &mut buf).unwrap();
        }
        assert_eq!(buf.capacity(), capacity, "capacity grew");
        assert!(std::ptr::eq(buf.as_ptr(), ptr), "buffer was reallocated");
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn a_length_mismatch_is_refused() {
        let skeleton = chain(3);
        let mut buf = Vec::new();
        let err = to_model_space_into(&skeleton, &[QsTransform::IDENTITY], &mut buf);
        assert!(err.is_err(), "a short pose must not be accepted");
    }

    #[test]
    fn a_parent_after_its_child_is_refused_rather_than_read_early() {
        let mut skeleton = chain(2);
        skeleton.parent_indices = vec![1, -1];
        let mut buf = Vec::new();
        let err = to_model_space_into(&skeleton, &[QsTransform::IDENTITY; 2], &mut buf);
        assert!(err.is_err(), "an out of order parent must not be accepted");
    }
}
