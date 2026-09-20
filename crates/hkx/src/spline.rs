// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoder for `hkaSplineCompressedAnimation` blocks.
//!
//! An animation is cut into blocks of at most `maxFramesPerBlock` frames.
//! Each block starts with one `TransformMask` per transform track (four
//! bytes: quantization kinds, then per-axis flags for position, rotation and
//! scale) and one byte per float track, padded to four. Then, per transform
//! track, the position, rotation and scale sub-tracks follow. A sub-track is
//! identity (absent), static (one value) or a B-spline over the block's
//! frames with `u8` knots and quantized control points.
//!
//! The layout and the quaternion packings were worked out from HavokLib's
//! `hka_spline_decompressor.cpp` (GPL-3.0, Lukas Cone), which is the
//! reference decoder for these files.

use crate::{Error, QsTransform, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantization {
    Bits8,
    Bits16,
    Bits32,
    Bits40,
    Bits48,
    Bits24,
    Bits16Quat,
    Uncompressed,
}

impl Quantization {
    fn from_index(i: u8) -> Result<Quantization> {
        Ok(match i {
            0 => Quantization::Bits8,
            1 => Quantization::Bits16,
            2 => Quantization::Bits32,
            3 => Quantization::Bits40,
            4 => Quantization::Bits48,
            5 => Quantization::Bits24,
            6 => Quantization::Bits16Quat,
            7 => Quantization::Uncompressed,
            n => return Err(Error::BadSpline(format!("quantization index {n}"))),
        })
    }
}

/// How a sub-track is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Identity,
    Static,
    Spline,
}

/// The four-byte per-track header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformMask {
    pub quantization: u8,
    pub position: u8,
    pub rotation: u8,
    pub scale: u8,
}

impl TransformMask {
    pub fn position_quantization(&self) -> Result<Quantization> {
        Quantization::from_index(self.quantization & 3)
    }
    pub fn rotation_quantization(&self) -> Result<Quantization> {
        Quantization::from_index(((self.quantization >> 2) & 0xF) + 2)
    }
    pub fn scale_quantization(&self) -> Result<Quantization> {
        Quantization::from_index((self.quantization >> 6) & 3)
    }

    /// Axis `axis` (0, 1, 2) of a position or scale flag byte. Bit `axis`
    /// means static, bit `axis + 4` means spline.
    fn axis_kind(flags: u8, axis: u8) -> TrackKind {
        if flags & (1 << axis) != 0 {
            TrackKind::Static
        } else if flags & (1 << (axis + 4)) != 0 {
            TrackKind::Spline
        } else {
            TrackKind::Identity
        }
    }
    pub fn position_kind(&self, axis: u8) -> TrackKind {
        Self::axis_kind(self.position, axis)
    }
    pub fn scale_kind(&self, axis: u8) -> TrackKind {
        Self::axis_kind(self.scale, axis)
    }
    pub fn rotation_kind(&self) -> TrackKind {
        if self.rotation & 0xF0 != 0 {
            TrackKind::Spline
        } else if self.rotation & 0x0F != 0 {
            TrackKind::Static
        } else {
            TrackKind::Identity
        }
    }
}

/// A three-axis sub-track. Each axis holds one value when static or
/// identity, and `n + 1` control points when it rides the spline.
#[derive(Debug, Clone, PartialEq)]
pub enum VecTrack {
    Static([f32; 3]),
    Spline {
        degree: u8,
        knots: Vec<u8>,
        axes: [Vec<f32>; 3],
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuatTrack {
    Static([f32; 4]),
    Spline {
        degree: u8,
        knots: Vec<u8>,
        points: Vec<[f32; 4]>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransformTrack {
    pub position: VecTrack,
    pub rotation: QuatTrack,
    pub scale: VecTrack,
}

/// A scalar animation track, independent of the transform-track order.
#[derive(Debug, Clone, PartialEq)]
pub enum FloatTrack {
    Static(f32),
    Spline {
        degree: u8,
        knots: Vec<u8>,
        points: Vec<f32>,
    },
}

impl FloatTrack {
    pub fn sample(&self, local_frame: f32) -> f32 {
        match self {
            Self::Static(value) => *value,
            Self::Spline {
                degree,
                knots,
                points,
            } => eval_scalar(*degree, knots, points, local_frame),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub masks: Vec<TransformMask>,
    pub float_masks: Vec<u8>,
    pub float_tracks: Vec<FloatTrack>,
    pub tracks: Vec<TransformTrack>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| Error::BadSpline(what.into()))?;
        if end > self.data.len() {
            return Err(Error::BadSpline(format!(
                "{what}: need {n} bytes at {}, block has {}",
                self.pos,
                self.data.len()
            )));
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self, what: &str) -> Result<u8> {
        Ok(self.take(1, what)?[0])
    }
    fn u16(&mut self, what: &str) -> Result<u16> {
        let b = self.take(2, what)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self, what: &str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self, what: &str) -> Result<f32> {
        Ok(f32::from_bits(self.u32(what)?))
    }
    /// Align relative to the start of the animation's data array, which
    /// Havok keeps 16-byte aligned in memory.
    fn align(&mut self, a: usize) {
        self.pos = self.pos.div_ceil(a) * a;
    }
}

const FRAC_10: f32 = 1.0 / 1023.0;
const PI: f32 = std::f32::consts::PI;

fn quat_32(v: u32) -> [f32; 4] {
    let r0 = ((v >> 18) & 0x3FF) as f32 * FRAC_10;
    let r = 1.0 - r0 * r0;
    let phi_theta = (v & 0x3FFFF) as f32;
    let mut phi = phi_theta.sqrt().floor();
    let mut theta = 0.0;
    if phi > 0.0 {
        theta = (PI / 4.0) * (phi_theta - phi * phi) / phi;
        phi *= (PI / 2.0) / 511.0;
    }
    let magnitude = (1.0 - r * r).max(0.0).sqrt();
    let (sp, cp) = (phi.sin(), phi.cos());
    let (st, ct) = (theta.sin(), theta.cos());
    let mut q = [sp * ct * magnitude, sp * st * magnitude, cp * magnitude, r];
    for (i, bit) in [0x1000_0000u32, 0x2000_0000, 0x4000_0000, 0x8000_0000]
        .iter()
        .enumerate()
    {
        if v & bit != 0 {
            q[i] = -q[i];
        }
    }
    q
}

/// Put the reconstructed component back where the dropped one was.
fn place_missing(xyz: [f32; 3], shift: u8, w_negative: bool) -> [f32; 4] {
    let sum: f32 = xyz.iter().map(|x| x * x).sum();
    let mut w = (1.0 - sum).max(0.0).sqrt();
    if w_negative {
        w = -w;
    }
    match shift {
        0 => [w, xyz[0], xyz[1], xyz[2]],
        1 => [xyz[0], w, xyz[1], xyz[2]],
        2 => [xyz[0], xyz[1], w, xyz[2]],
        _ => [xyz[0], xyz[1], xyz[2], w],
    }
}

fn quat_40(b: &[u8]) -> [f32; 4] {
    let mut raw = [0u8; 8];
    raw[..5].copy_from_slice(&b[..5]);
    let v = u64::from_le_bytes(raw);
    const MASK: i64 = (1 << 11) - 1;
    const FRAC: f32 = 0.000345436;
    let x = ((v & 0xFFF) as i64 - MASK) as f32 * FRAC;
    let y = (((v >> 12) & 0xFFF) as i64 - MASK) as f32 * FRAC;
    let z = (((v >> 24) & 0xFFF) as i64 - MASK) as f32 * FRAC;
    let shift = ((v >> 36) & 3) as u8;
    let w_negative = (v >> 38) & 1 == 1;
    place_missing([x, y, z], shift, w_negative)
}

fn quat_48(b: &[u8]) -> [f32; 4] {
    let x = u16::from_le_bytes([b[0], b[1]]);
    let y = u16::from_le_bytes([b[2], b[3]]);
    let z = u16::from_le_bytes([b[4], b[5]]);
    const MASK: i32 = (1 << 15) - 1;
    const FRAC: f32 = 0.000043161;
    let shift = (((y >> 14) & 2) | ((x >> 15) & 1)) as u8;
    let w_negative = (z >> 15) != 0;
    let dq = |v: u16| ((v as i32 & MASK) - (MASK >> 1)) as f32 * FRAC;
    place_missing([dq(x), dq(y), dq(z)], shift, w_negative)
}

fn read_quat(r: &mut Reader<'_>, q: Quantization) -> Result<[f32; 4]> {
    Ok(match q {
        Quantization::Bits32 => quat_32(r.u32("quaternion")?),
        Quantization::Bits40 => quat_40(r.take(5, "quaternion")?),
        Quantization::Bits48 => quat_48(r.take(6, "quaternion")?),
        Quantization::Uncompressed => {
            let b = r.take(16, "quaternion")?;
            let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
            [f(0), f(4), f(8), f(12)]
        }
        other => {
            return Err(Error::Unsupported(format!(
                "{other:?} rotation quantization"
            )))
        }
    })
}

/// Algorithm A2.1 of The NURBS Book: the knot span holding `t`.
fn find_knot_span(degree: usize, t: f32, num_points: usize, knots: &[u8]) -> usize {
    if t >= knots[num_points] as f32 {
        return num_points - 1;
    }
    let mut low = degree;
    let mut high = num_points;
    let mut mid = low.midpoint(high);
    while t < knots[mid] as f32 || t >= knots[mid + 1] as f32 {
        if t < knots[mid] as f32 {
            high = mid;
        } else {
            low = mid;
        }
        let next = low.midpoint(high);
        if next == mid {
            break;
        }
        mid = next;
    }
    mid
}

/// Basis functions at `t` for the span, then the weighted control points.
fn basis(span: usize, degree: usize, t: f32, knots: &[u8]) -> [f32; 5] {
    let mut n = [0.0f32; 5];
    n[0] = 1.0;
    for i in 1..=degree {
        for j in (0..i).rev() {
            let k0 = knots[span - j] as f32;
            let k1 = knots[span + i - j] as f32;
            let a = if k1 == k0 { 0.0 } else { (t - k0) / (k1 - k0) };
            let tmp = n[j] * a;
            n[j + 1] += n[j] - tmp;
            n[j] = tmp;
        }
    }
    n
}

fn eval_scalar(degree: u8, knots: &[u8], points: &[f32], t: f32) -> f32 {
    if points.len() == 1 {
        return points[0];
    }
    let d = degree as usize;
    let span = find_knot_span(d, t, points.len(), knots);
    let n = basis(span, d, t, knots);
    (0..=d).map(|i| points[span - i] * n[i]).sum()
}

fn eval_quat(degree: u8, knots: &[u8], points: &[[f32; 4]], t: f32) -> [f32; 4] {
    let d = degree as usize;
    let span = find_knot_span(d, t, points.len(), knots);
    let n = basis(span, d, t, knots);
    let mut q = [0.0f32; 4];
    for i in 0..=d {
        for (c, p) in q.iter_mut().zip(points[span - i].iter()) {
            *c += p * n[i];
        }
    }
    let len = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    if len > 0.0 {
        for c in &mut q {
            *c /= len;
        }
    }
    q
}

impl VecTrack {
    pub fn sample(&self, local_frame: f32) -> [f32; 3] {
        match self {
            VecTrack::Static(v) => *v,
            VecTrack::Spline {
                degree,
                knots,
                axes,
            } => [
                eval_scalar(*degree, knots, &axes[0], local_frame),
                eval_scalar(*degree, knots, &axes[1], local_frame),
                eval_scalar(*degree, knots, &axes[2], local_frame),
            ],
        }
    }
}

impl QuatTrack {
    pub fn sample(&self, local_frame: f32) -> [f32; 4] {
        match self {
            QuatTrack::Static(q) => *q,
            QuatTrack::Spline {
                degree,
                knots,
                points,
            } => eval_quat(*degree, knots, points, local_frame),
        }
    }
}

impl TransformTrack {
    pub fn sample(&self, local_frame: f32) -> QsTransform {
        let p = self.position.sample(local_frame);
        let s = self.scale.sample(local_frame);
        QsTransform {
            translation: [p[0], p[1], p[2], 0.0],
            rotation: self.rotation.sample(local_frame),
            scale: [s[0], s[1], s[2], 0.0],
        }
    }
}

/// Read a spline header: item count, degree, knots. Returns
/// `(num_items, degree, knots)`, where the control point count is
/// `num_items + 1`.
fn read_spline_header(r: &mut Reader<'_>) -> Result<(usize, u8, Vec<u8>)> {
    let num_items = r.u16("spline item count")? as usize;
    let degree = r.u8("spline degree")?;
    if degree == 0 || degree > 4 {
        return Err(Error::BadSpline(format!("spline degree {degree}")));
    }
    let knots = r
        .take(num_items + degree as usize + 2, "spline knots")?
        .to_vec();
    Ok((num_items, degree, knots))
}

fn read_vec_track(
    r: &mut Reader<'_>,
    kinds: [TrackKind; 3],
    q: Quantization,
    default: f32,
) -> Result<VecTrack> {
    let any_spline = kinds.contains(&TrackKind::Spline);
    if !any_spline {
        let mut v = [default; 3];
        for (axis, kind) in kinds.iter().enumerate() {
            if *kind == TrackKind::Static {
                v[axis] = r.f32("static component")?;
            }
        }
        return Ok(VecTrack::Static(v));
    }
    let (num_items, degree, knots) = read_spline_header(r)?;
    r.align(4);
    let mut axes: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut extremes = [[0.0f32; 2]; 3];
    for axis in 0..3 {
        match kinds[axis] {
            TrackKind::Spline => {
                extremes[axis] = [r.f32("spline min")?, r.f32("spline max")?];
                axes[axis] = vec![0.0; num_items + 1];
            }
            TrackKind::Static => axes[axis].push(r.f32("static component")?),
            TrackKind::Identity => axes[axis].push(default),
        }
    }
    // Control points are interleaved point-major across the axes, so the
    // read order does not fit a per-axis iterator.
    #[allow(clippy::needless_range_loop)]
    for point in 0..=num_items {
        for axis in 0..3 {
            if kinds[axis] != TrackKind::Spline {
                continue;
            }
            let frac = match q {
                Quantization::Bits8 => r.u8("8-bit control point")? as f32 / 255.0,
                Quantization::Bits16 => r.u16("16-bit control point")? as f32 / 65535.0,
                other => {
                    return Err(Error::Unsupported(format!(
                        "{other:?} position or scale quantization"
                    )))
                }
            };
            let [lo, hi] = extremes[axis];
            axes[axis][point] = lo + (hi - lo) * frac;
        }
    }
    r.align(4);
    Ok(VecTrack::Spline {
        degree,
        knots,
        axes,
    })
}

fn read_quat_track(r: &mut Reader<'_>, mask: TransformMask) -> Result<QuatTrack> {
    let q = mask.rotation_quantization()?;
    match mask.rotation_kind() {
        TrackKind::Identity => Ok(QuatTrack::Static([0.0, 0.0, 0.0, 1.0])),
        TrackKind::Static => Ok(QuatTrack::Static(read_quat(r, q)?)),
        TrackKind::Spline => {
            let (num_items, degree, knots) = read_spline_header(r)?;
            match q {
                Quantization::Bits48 | Quantization::Bits16Quat => r.align(2),
                Quantization::Bits32 | Quantization::Uncompressed => r.align(4),
                _ => {}
            }
            let mut points = Vec::with_capacity(num_items + 1);
            for _ in 0..=num_items {
                points.push(read_quat(r, q)?);
            }
            Ok(QuatTrack::Spline {
                degree,
                knots,
                points,
            })
        }
    }
}

fn read_float_track(r: &mut Reader<'_>, mask: u8) -> Result<FloatTrack> {
    let quantization = (mask >> 1) & 3;
    if quantization > 1 || mask & !0x17 != 0 || mask & 0x11 == 0x11 {
        return Err(Error::BadSpline(format!("float mask {mask:#04x}")));
    }
    if mask & 0x10 == 0 {
        let value = if mask & 1 != 0 {
            r.f32("static float")?
        } else {
            0.0
        };
        if !value.is_finite() {
            return Err(Error::BadSpline("non-finite static float".into()));
        }
        return Ok(FloatTrack::Static(value));
    }
    let (items, degree, knots) = read_spline_header(r)?;
    let count = items + 1;
    if degree > 3
        || count <= usize::from(degree)
        || knots.windows(2).any(|pair| pair[0] > pair[1])
        || knots[usize::from(degree)] >= knots[count]
    {
        return Err(Error::BadSpline(
            "invalid float spline knots or degree".into(),
        ));
    }
    r.align(4);
    let lo = r.f32("float minimum")?;
    let hi = r.f32("float maximum")?;
    if !lo.is_finite() || !hi.is_finite() || lo > hi {
        return Err(Error::BadSpline("invalid float spline bounds".into()));
    }
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let fraction = if quantization == 0 {
            f64::from(r.u8("8-bit float point")?) / 255.0
        } else {
            f64::from(r.u16("16-bit float point")?) / 65535.0
        };
        points.push((f64::from(lo) + (f64::from(hi) - f64::from(lo)) * fraction) as f32);
    }
    r.align(4);
    Ok(FloatTrack::Spline {
        degree,
        knots,
        points,
    })
}

/// Decode one block. `data` is the animation's whole data array and
/// `offset` the block's start in it, so alignment matches Havok's.
pub fn decode_block(
    data: &[u8],
    offset: usize,
    num_transform_tracks: usize,
    num_float_tracks: usize,
) -> Result<Block> {
    decode_block_with_float_offset(data, offset, num_transform_tracks, num_float_tracks, None)
}

/// Decode using an optional block-relative start for the float payload.
pub(crate) fn decode_block_with_float_offset(
    data: &[u8],
    offset: usize,
    num_transform_tracks: usize,
    num_float_tracks: usize,
    float_offset: Option<usize>,
) -> Result<Block> {
    if offset > data.len() {
        return Err(Error::BadSpline(format!(
            "block offset {offset} past data of {}",
            data.len()
        )));
    }
    let mut r = Reader { data, pos: offset };
    let mut masks = Vec::with_capacity(num_transform_tracks);
    for _ in 0..num_transform_tracks {
        let b = r.take(4, "transform mask")?;
        masks.push(TransformMask {
            quantization: b[0],
            position: b[1],
            rotation: b[2],
            scale: b[3],
        });
    }
    let float_masks = r.take(num_float_tracks, "float masks")?.to_vec();
    r.align(4);
    let mut tracks = Vec::with_capacity(num_transform_tracks);
    for m in &masks {
        let position = read_vec_track(
            &mut r,
            [m.position_kind(0), m.position_kind(1), m.position_kind(2)],
            m.position_quantization()?,
            0.0,
        )?;
        let rotation = read_quat_track(&mut r, *m)?;
        r.align(4);
        let scale = read_vec_track(
            &mut r,
            [m.scale_kind(0), m.scale_kind(1), m.scale_kind(2)],
            m.scale_quantization()?,
            1.0,
        )?;
        tracks.push(TransformTrack {
            position,
            rotation,
            scale,
        });
    }
    if num_float_tracks > 0 {
        r.align(4);
        if let Some(relative) = float_offset {
            let start = offset
                .checked_add(relative)
                .filter(|&start| start >= r.pos && start <= data.len())
                .ok_or_else(|| Error::BadSpline("invalid float block offset".into()))?;
            r.pos = start;
        }
    }
    let float_tracks = float_masks
        .iter()
        .map(|&mask| read_float_track(&mut r, mask))
        .collect::<Result<Vec<_>>>()?;
    Ok(Block {
        masks,
        float_masks,
        float_tracks,
        tracks,
    })
}

impl Block {
    pub fn sample(&self, track: usize, local_frame: f32) -> Option<QsTransform> {
        self.tracks.get(track).map(|t| t.sample(local_frame))
    }
}

/// Encoders used by the tests to build blocks the decoder must read back.
/// They are the inverse of the decoders above and are public so other
/// crates' tests can make animations too.
pub mod encode {
    /// Quantize a control point into `[lo, hi]` at 16 bits.
    pub fn point_16(lo: f32, hi: f32, v: f32) -> u16 {
        if hi == lo {
            return 0;
        }
        (((v - lo) / (hi - lo)).clamp(0.0, 1.0) * 65535.0).round() as u16
    }

    /// Pack a unit quaternion into the 48-bit form: the largest component
    /// is dropped and its sign carried in the z slot's top bit.
    pub fn quat_48(q: [f32; 4]) -> [u8; 6] {
        // CONTEXT: `0..4` is never empty, so `max_by` always returns `Some`.
        #[allow(clippy::unwrap_used)]
        let largest = (0..4)
            .max_by(|&a, &b| q[a].abs().total_cmp(&q[b].abs()))
            .unwrap();
        let sign_negative = q[largest] < 0.0;
        let rest: Vec<f32> = (0..4).filter(|&i| i != largest).map(|i| q[i]).collect();
        const MASK: i32 = (1 << 15) - 1;
        const FRAC: f32 = 0.000043161;
        let enc = |v: f32| ((v / FRAC).round() as i32 + (MASK >> 1)).clamp(0, MASK) as u16;
        let mut x = enc(rest[0]);
        let mut y = enc(rest[1]);
        let mut z = enc(rest[2]);
        let shift = largest as u16;
        x |= (shift & 1) << 15;
        y |= ((shift >> 1) & 1) << 15;
        if sign_negative {
            z |= 1 << 15;
        }
        let mut out = [0u8; 6];
        out[..2].copy_from_slice(&x.to_le_bytes());
        out[2..4].copy_from_slice(&y.to_le_bytes());
        out[4..].copy_from_slice(&z.to_le_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    fn pad4(v: &mut Vec<u8>) {
        while !v.len().is_multiple_of(4) {
            v.push(0);
        }
    }

    /// One track: x position on a degree-1 spline through 0, 10, 20 over
    /// frames 0..2, static y = 5, identity z. Rotation identity, scale
    /// identity.
    fn linear_block() -> Vec<u8> {
        let mut b = Vec::new();
        // mask: pos quant 16-bit (1), rot quant index 2 -> 32-bit but
        // identity, scale quant 0.
        b.extend_from_slice(&[1, 0b0001_0010, 0, 0]);
        pad4(&mut b);
        // spline header: 2 items (3 points), degree 1, knots 0 0 1 2 2
        b.extend_from_slice(&2u16.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&[0, 0, 1, 2, 2]);
        pad4(&mut b);
        // x extremes then static y
        b.extend_from_slice(&0.0f32.to_le_bytes());
        b.extend_from_slice(&20.0f32.to_le_bytes());
        b.extend_from_slice(&5.0f32.to_le_bytes());
        for v in [0.0f32, 10.0, 20.0] {
            b.extend_from_slice(&encode::point_16(0.0, 20.0, v).to_le_bytes());
        }
        pad4(&mut b);
        b
    }

    #[test]
    fn a_linear_spline_interpolates_between_its_control_points() {
        let data = linear_block();
        let block = decode_block(&data, 0, 1, 0).unwrap();
        let t = &block.tracks[0];
        for (frame, want) in [
            (0.0f32, 0.0f32),
            (0.5, 5.0),
            (1.0, 10.0),
            (1.5, 15.0),
            (2.0, 20.0),
        ] {
            let p = t.position.sample(frame);
            assert!((p[0] - want).abs() < 0.01, "frame {frame}: {p:?}");
            assert_eq!(p[1], 5.0);
            assert_eq!(p[2], 0.0);
        }
        let s = t.sample(1.0);
        assert_eq!(s.rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(s.scale, [1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn a_quadratic_spline_matches_de_boor_by_hand() {
        // Degree 2, control points 0, 4, 8, 4 with knots 0 0 0 1 2 2 2.
        // At t = 0.5 the basis on span 2 is (0.25, 0.625, 0.125) over
        // points (P2, P1, P0) ... computed by hand as 0.5^2/2 etc:
        // N = [(1-t)^2, ...] for the open uniform case gives
        // value = 0.25*0 + 0.625*4 + 0.125*8 = 3.5.
        let knots = [0u8, 0, 0, 1, 2, 2, 2];
        let points = [0.0f32, 4.0, 8.0, 4.0];
        let v = eval_scalar(2, &knots, &points, 0.5);
        assert!((v - 3.5).abs() < 1e-5, "{v}");
        // At t = 1 (the interior knot) the curve passes through the
        // midpoint-weighted blend: (P1 + P2) / 2 = 6.
        let v = eval_scalar(2, &knots, &points, 1.0);
        assert!((v - 6.0).abs() < 1e-5, "{v}");
        // At the end it hits the last control point.
        let v = eval_scalar(2, &knots, &points, 2.0);
        assert!((v - 4.0).abs() < 1e-5, "{v}");
    }

    #[test]
    fn the_48_bit_quaternion_round_trips() {
        for q in [
            [0.0f32, 0.0, 0.0, 1.0],
            [0.0, 0.0, FRAC_1_SQRT_2, FRAC_1_SQRT_2],
            [FRAC_1_SQRT_2, 0.0, 0.0, -FRAC_1_SQRT_2],
            [0.1, 0.2, 0.3, 0.927362],
        ] {
            let packed = encode::quat_48(q);
            let got = quat_48(&packed);
            for i in 0..4 {
                assert!((got[i] - q[i]).abs() < 2e-3, "{q:?} -> {got:?}");
            }
        }
    }

    #[test]
    fn the_32_bit_identity_decodes_to_the_identity() {
        // R field all ones gives r0 = 1 -> r = 0, so w = 0 ... instead take
        // R = 0: r = 1, magnitude 0, quaternion (0, 0, 0, 1).
        let q = quat_32(0);
        assert_eq!(q, [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_truncated_block_is_an_error() {
        let data = linear_block();
        let cut = &data[..data.len() - 4];
        assert!(matches!(
            decode_block(cut, 0, 1, 0),
            Err(Error::BadSpline(_))
        ));
        assert!(matches!(
            decode_block(&data, data.len() + 1, 1, 0),
            Err(Error::BadSpline(_))
        ));
    }

    #[test]
    fn a_zero_degree_spline_is_rejected() {
        let mut b = Vec::new();
        b.extend_from_slice(&[1, 0b0001_0000, 0, 0]);
        b.extend_from_slice(&2u16.to_le_bytes());
        b.push(0);
        b.extend_from_slice(&[0; 16]);
        assert!(matches!(
            decode_block(&b, 0, 1, 0),
            Err(Error::BadSpline(_))
        ));
    }
}
