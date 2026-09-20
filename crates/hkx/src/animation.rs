// SPDX-License-Identifier: GPL-3.0-or-later

//! `hkaAnimation` and its one Skyrim SE representative,
//! `hkaSplineCompressedAnimation`. Interleaved, delta and wavelet animations
//! never ship in Skyrim files and are reported as unsupported.

use crate::packfile::Obj;
use crate::spline::{decode_block_with_float_offset, Block};
use crate::{Error, QsTransform, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub time: f32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationTrack {
    pub track_name: String,
    pub annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Animation {
    SplineCompressed(SplineCompressedAnimation),
    Unsupported {
        class: String,
        animation_type: i32,
        duration: f32,
    },
}

impl Animation {
    pub fn from_obj(o: Obj<'_>) -> Result<Animation> {
        match o.class() {
            Some("hkaSplineCompressedAnimation") => Ok(Animation::SplineCompressed(
                SplineCompressedAnimation::from_obj(o)?,
            )),
            Some(other) => Ok(Animation::Unsupported {
                class: other.to_string(),
                animation_type: o.i32(o.layout_offset(8, 16))?,
                duration: o.f32(o.layout_offset(12, 20))?,
            }),
            None => Err(Error::WrongClass {
                expected: "hkaAnimation",
                found: "<unnamed>".into(),
            }),
        }
    }

    pub fn duration(&self) -> f32 {
        match self {
            Animation::SplineCompressed(s) => s.duration,
            Animation::Unsupported { duration, .. } => *duration,
        }
    }
}

/// Layout of `hkaAnimation` (base): type 16, duration 20,
/// numberOfTransformTracks 24, numberOfFloatTracks 28, extractedMotion 32,
/// annotationTracks 40 (24-byte elements: trackName 0, annotations 8 with
/// 16-byte elements: time 0, text 8). Then the spline fields: numFrames 56,
/// numBlocks 60, maxFramesPerBlock 64, maskAndQuantizationSize 68,
/// blockDuration 72, blockInverseDuration 76, frameDuration 80,
/// blockOffsets 88, floatBlockOffsets 104, transformOffsets 120,
/// floatOffsets 136, data 152, endian 168.
#[derive(Debug, Clone, PartialEq)]
pub struct SplineCompressedAnimation {
    pub duration: f32,
    pub num_transform_tracks: usize,
    pub num_float_tracks: usize,
    pub annotation_tracks: Vec<AnnotationTrack>,
    pub num_frames: usize,
    pub num_blocks: usize,
    pub max_frames_per_block: usize,
    pub mask_and_quantization_size: i32,
    pub block_duration: f32,
    pub block_inverse_duration: f32,
    pub frame_duration: f32,
    pub block_offsets: Vec<u32>,
    pub float_block_offsets: Vec<u32>,
    pub transform_offsets: Vec<u32>,
    pub float_offsets: Vec<u32>,
    pub data: Vec<u8>,
    pub endian: i32,
    /// Whether the extracted-motion pointer was set.
    pub has_extracted_motion: bool,
}

fn non_negative(v: i32, what: &str) -> Result<usize> {
    usize::try_from(v).map_err(|_| Error::BadSpline(format!("{what} is {v}")))
}

impl SplineCompressedAnimation {
    pub fn from_obj(o: Obj<'_>) -> Result<SplineCompressedAnimation> {
        o.expect_class("hkaSplineCompressedAnimation")?;
        let annotation_tracks = o
            .array_structs(o.layout_offset(28, 40), o.layout_offset(16, 24))?
            .into_iter()
            .map(|t| {
                Ok(AnnotationTrack {
                    track_name: t.string_or_empty(0)?,
                    annotations: t
                        .array_structs(t.layout_offset(4, 8), t.layout_offset(8, 16))?
                        .into_iter()
                        .map(|a| {
                            Ok(Annotation {
                                time: a.f32(0)?,
                                text: a.string_or_empty(a.layout_offset(4, 8))?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let anim = SplineCompressedAnimation {
            duration: o.f32(o.layout_offset(12, 20))?,
            num_transform_tracks: non_negative(
                o.i32(o.layout_offset(16, 24))?,
                "numberOfTransformTracks",
            )?,
            num_float_tracks: non_negative(o.i32(o.layout_offset(20, 28))?, "numberOfFloatTracks")?,
            annotation_tracks,
            num_frames: non_negative(o.i32(o.layout_offset(40, 56))?, "numFrames")?,
            num_blocks: non_negative(o.i32(o.layout_offset(44, 60))?, "numBlocks")?,
            max_frames_per_block: non_negative(
                o.i32(o.layout_offset(48, 64))?,
                "maxFramesPerBlock",
            )?,
            mask_and_quantization_size: o.i32(o.layout_offset(52, 68))?,
            block_duration: o.f32(o.layout_offset(56, 72))?,
            block_inverse_duration: o.f32(o.layout_offset(60, 76))?,
            frame_duration: o.f32(o.layout_offset(64, 80))?,
            block_offsets: o.array_u32(o.layout_offset(68, 88))?,
            float_block_offsets: o.array_u32(o.layout_offset(80, 104))?,
            transform_offsets: o.array_u32(o.layout_offset(92, 120))?,
            float_offsets: o.array_u32(o.layout_offset(104, 136))?,
            data: o.array_u8(o.layout_offset(116, 152))?,
            endian: o.i32(o.layout_offset(128, 168))?,
            has_extracted_motion: o.ptr(o.layout_offset(24, 32))?.is_some(),
        };
        if anim.block_offsets.len() != anim.num_blocks {
            return Err(Error::BadSpline(format!(
                "{} block offsets for {} blocks",
                anim.block_offsets.len(),
                anim.num_blocks
            )));
        }
        if anim.max_frames_per_block == 0 && anim.num_blocks > 0 {
            return Err(Error::BadSpline("maxFramesPerBlock is 0".into()));
        }
        anim.validate_timing()?;
        Ok(anim)
    }

    fn validate_timing(&self) -> Result<()> {
        let values = [
            self.duration,
            self.frame_duration,
            self.block_duration,
            self.block_inverse_duration,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || (self.num_frames > 1 && values[1..].contains(&0.0))
            || !((self.num_frames.saturating_sub(1)) as f32 * self.frame_duration).is_finite()
        {
            return Err(Error::BadSpline("invalid animation timing".into()));
        }
        Ok(())
    }

    /// Decode every block.
    pub fn decode(&self) -> Result<Vec<Block>> {
        self.validate_timing()?;
        if self.num_float_tracks > 0 && self.float_block_offsets.len() != self.num_blocks {
            return Err(Error::BadSpline("missing float block offsets".into()));
        }
        self.block_offsets
            .iter()
            .enumerate()
            .map(|(index, &off)| {
                let end = self
                    .block_offsets
                    .get(index + 1)
                    .map_or(self.data.len(), |&next| next as usize);
                let data = self
                    .data
                    .get(..end)
                    .filter(|_| off as usize <= end)
                    .ok_or_else(|| Error::BadSpline("invalid block boundary".into()))?;
                decode_block_with_float_offset(
                    data,
                    off as usize,
                    self.num_transform_tracks,
                    self.num_float_tracks,
                    self.float_block_offsets
                        .get(index)
                        .map(|&offset| offset as usize),
                )
            })
            .collect()
    }

    /// One float track at a time, from blocks produced by [`Self::decode`].
    pub fn sample_float(&self, blocks: &[Block], track: usize, time: f32) -> Option<f32> {
        let (block, local) = self.locate(time);
        Some(blocks.get(block)?.float_tracks.get(track)?.sample(local))
    }

    /// Which block holds a time, and the frame within it. Times outside the
    /// animation clamp to its ends.
    pub fn locate(&self, time: f32) -> (usize, f32) {
        if self.num_frames == 0
            || self.frame_duration <= 0.0
            || self.num_blocks == 0
            || self.validate_timing().is_err()
        {
            return (0, 0.0);
        }
        let last = (self.num_frames - 1) as f32;
        let time = time.clamp(0.0, last * self.frame_duration);
        // Authored block duration includes the shared boundary frame.
        let block = ((time * self.block_inverse_duration) as usize).min(self.num_blocks - 1);
        let local = (time - block as f32 * self.block_duration).max(0.0);
        (block, local / self.frame_duration)
    }

    /// One track at a time, from blocks produced by [`Self::decode`].
    pub fn sample(&self, blocks: &[Block], track: usize, time: f32) -> Option<QsTransform> {
        let (block, local) = self.locate(time);
        blocks.get(block)?.sample(track, local)
    }

    /// Every transform track at a time.
    pub fn sample_pose(&self, blocks: &[Block], time: f32) -> Vec<QsTransform> {
        let (block, local) = self.locate(time);
        match blocks.get(block) {
            Some(b) => b.tracks.iter().map(|t| t.sample(local)).collect(),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(frames: usize, per_block: usize, blocks: usize) -> SplineCompressedAnimation {
        SplineCompressedAnimation {
            duration: (frames as f32 - 1.0) / 30.0,
            num_transform_tracks: 0,
            num_float_tracks: 0,
            annotation_tracks: vec![],
            num_frames: frames,
            num_blocks: blocks,
            max_frames_per_block: per_block,
            mask_and_quantization_size: 0,
            block_duration: (per_block - 1) as f32 / 30.0,
            block_inverse_duration: 30.0 / (per_block - 1) as f32,
            frame_duration: 1.0 / 30.0,
            block_offsets: vec![0; blocks],
            float_block_offsets: vec![],
            transform_offsets: vec![],
            float_offsets: vec![],
            data: vec![],
            endian: 0,
            has_extracted_motion: false,
        }
    }

    #[test]
    fn times_map_to_blocks_and_local_frames() {
        let a = anim(63, 32, 2);
        assert_eq!(a.locate(0.0), (0, 0.0));
        let (b, f) = a.locate(31.0 / 30.0);
        assert!((b as f32 * 31.0 + f - 31.0).abs() < 1e-4);
        let (b, f) = a.locate(31.5 / 30.0);
        assert_eq!(b, 1);
        assert!((f - 0.5).abs() < 1e-4);
        let (b, f) = a.locate(40.0 / 30.0);
        assert_eq!(b, 1);
        assert!((f - 9.0).abs() < 1e-4);
        // Past the end clamps to the last frame of the last block.
        let (b, f) = a.locate(100.0);
        assert_eq!(b, 1);
        assert!((f - 31.0).abs() < 1e-4);
        // Before the start clamps to zero.
        assert_eq!(a.locate(-1.0), (0, 0.0));
    }
}
