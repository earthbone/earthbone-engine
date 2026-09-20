// SPDX-License-Identifier: GPL-3.0-or-later

//! Extracted motion from `hkaDefaultAnimatedReferenceFrame`. Layout follows
//! serde-hkx's class table. Sampling follows HavokLib `core_motion.cpp`:
//! linear translation and spherical rotation interpolation about authored up.

use crate::{Error, Obj, QsTransform, Result};

/// Uniformly spaced character translations and heading angles in radians.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceFrame {
    up: [f32; 4],
    forward: [f32; 4],
    duration: f32,
    samples: Vec<[f32; 4]>,
}

impl ReferenceFrame {
    /// Decode the SE reference frame at an animation's extracted-motion pointer.
    ///
    /// # Errors
    /// Rejects other frame classes, truncated fields, non-finite values,
    /// non-unit up vectors and multiple samples without a positive duration.
    pub fn from_obj(o: Obj<'_>) -> Result<Self> {
        o.expect_class("hkaDefaultAnimatedReferenceFrame")?;
        let frame = Self {
            up: o.vec4(16)?,
            forward: o.vec4(32)?,
            duration: o.f32(48)?,
            samples: o.array_vec4(o.layout_offset(52, 56))?,
        };
        if !frame.duration.is_finite()
            || frame.duration < 0.0
            || (frame.samples.len() > 1 && frame.duration == 0.0)
        {
            return Err(Error::BadMotion("invalid reference-frame duration".into()));
        }
        if !frame.up.iter().chain(&frame.forward).all(|v| v.is_finite())
            || !frame.samples.iter().flatten().all(|v| v.is_finite())
        {
            return Err(Error::BadMotion("non-finite reference-frame data".into()));
        }
        let norm: f32 = frame.up[..3].iter().map(|v| v * v).sum();
        if (norm - 1.0).abs() > 1e-3 {
            return Err(Error::BadMotion(
                "reference-frame up is not unit length".into(),
            ));
        }
        Ok(frame)
    }

    /// Axis used for the authored heading rotations.
    pub fn up(&self) -> [f32; 4] {
        self.up
    }

    /// Authored forward direction, retained in file coordinates.
    pub fn forward(&self) -> [f32; 4] {
        self.forward
    }

    /// Duration in seconds, independent of the transform-track frame count.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Original samples, with translation in xyz and heading angle in w.
    pub fn samples(&self) -> &[[f32; 4]] {
        &self.samples
    }

    /// Sample absolute extracted motion, clamping time to the frame's ends.
    /// Empty frames yield identity and single-sample frames are constant.
    ///
    /// # Errors
    /// Rejects non-finite sample times.
    pub fn sample(&self, time: f32) -> Result<QsTransform> {
        if !time.is_finite() {
            return Err(Error::BadMotion("non-finite sample time".into()));
        }
        let Some(first) = self.samples.first() else {
            return Ok(QsTransform::IDENTITY);
        };
        let (a, b, fraction) = if self.samples.len() == 1 {
            (first, first, 0.0)
        } else {
            let position = (time / self.duration).clamp(0.0, 1.0) * (self.samples.len() - 1) as f32;
            let index = (position as usize).min(self.samples.len() - 1);
            let next = (index + 1).min(self.samples.len() - 1);
            (
                &self.samples[index],
                &self.samples[next],
                position - index as f32,
            )
        };
        let mut result = QsTransform::IDENTITY;
        for axis in 0..3 {
            result.translation[axis] = a[axis] * (1.0 - fraction) + b[axis] * fraction;
        }
        // CONTEXT: Same-axis spherical interpolation takes the shortest arc.
        let difference = f64::from(b[3]) - f64::from(a[3]);
        let angle =
            f64::from(a[3]) + difference.sin().atan2(difference.cos()) * f64::from(fraction);
        let (sin, cos) = (angle * 0.5).sin_cos();
        result.rotation = [
            self.up[0] * sin as f32,
            self.up[1] * sin as f32,
            self.up[2] * sin as f32,
            cos as f32,
        ];
        Ok(result)
    }
}
