// SPDX-License-Identifier: GPL-3.0-or-later

//! Transform controllers: the `NiTimeController` family that moves a node's
//! transform over time. Covers the plain `NiTransformController` path
//! (`NiTransformInterpolator` over a `NiTransformData` key track), which is
//! what Skyrim SE's animated statics use. `NiPathInterpolator` is parsed but
//! its curve-follow evaluation is not implemented yet, see `docs/nif.md`.
//!
//! Field layouts come from `nif.xml` (niftools) at Bethesda stream 100/83,
//! read directly rather than through any generated binding. Cycle mode and
//! key interpolation math are grounded on OpenMW's
//! `components/nifosg/controller.cpp` (`ControllerFunction::calculate`) and
//! `components/nif/data.hpp` (`KeyMapT::interpKey`), read from
//! `../engine-refs/openmw`.

use crate::error::{Error, Result};
use crate::stream::Stream;
use crate::types::{BlockRef, Mat33, Quat, Transform, Vec3};

/// How a controller's time wraps once it runs past `stop_time`.
/// `nif.xml` line 1022: `CYCLE_LOOP=0`, `CYCLE_REVERSE=1`, `CYCLE_CLAMP=2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CycleType {
    Loop,
    Reverse,
    #[default]
    Clamp,
}

impl CycleType {
    fn from_bits(bits: u16) -> CycleType {
        match bits {
            0 => CycleType::Loop,
            1 => CycleType::Reverse,
            _ => CycleType::Clamp,
        }
    }
}

/// `TimeControllerFlags`, `nif.xml` lines 1543-1552. Cycle type sits in bits
/// 1 to 2, the rest are single bits.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ControllerFlags {
    pub anim_type_backwards: bool,
    pub cycle_type: CycleType,
    pub active: bool,
    pub play_backwards: bool,
    pub manager_controlled: bool,
    pub compute_scaled_time: bool,
    pub forced_update: bool,
}

impl ControllerFlags {
    fn from_bits(bits: u16) -> ControllerFlags {
        ControllerFlags {
            anim_type_backwards: bits & 0x1 != 0,
            cycle_type: CycleType::from_bits((bits & 0x6) >> 1),
            active: bits & 0x8 != 0,
            play_backwards: bits & 0x10 != 0,
            manager_controlled: bits & 0x20 != 0,
            compute_scaled_time: bits & 0x40 != 0,
            forced_update: bits & 0x80 != 0,
        }
    }
}

/// `NiTimeController` plus the `NiSingleInterpController` interpolator ref,
/// which is the whole of `NiTransformController` and `NiKeyframeController`
/// at file version 20.2.0.7: neither adds a field of its own there
/// (`nif.xml` lines 3600-3665).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TimeController {
    pub next_controller: BlockRef,
    pub flags: ControllerFlags,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub target: BlockRef,
}

/// Material channel selected by a `NiMaterialColorController`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaterialColorTarget {
    #[default]
    Ambient,
    Diffuse,
    Specular,
    SelfIllumination,
}

/// The material channels evaluated by one authored material controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialColors {
    pub ambient: Vec3,
    pub diffuse: Vec3,
    pub specular: Vec3,
    pub self_illumination: Vec3,
}

impl MaterialColors {
    /// Apply one sampled color while preserving all other material channels.
    pub fn set(&mut self, target: MaterialColorTarget, value: Vec3) {
        match target {
            MaterialColorTarget::Ambient => self.ambient = value,
            MaterialColorTarget::Diffuse => self.diffuse = value,
            MaterialColorTarget::Specular => self.specular = value,
            MaterialColorTarget::SelfIllumination => self.self_illumination = value,
        }
    }
}

/// Sample an authored `NiMaterialColorController` track and update its one
/// selected color channel. `None` means the track has no keys and the
/// material remains unchanged.
pub fn evaluate_material_color(
    time: &TimeController,
    target: MaterialColorTarget,
    colors: &mut MaterialColors,
    track: &KeyGroup<Vec3>,
    game_time: f32,
) -> bool {
    let local_time = evaluate_time(time, game_time);
    let Some(value) = evaluate_track(track, local_time) else {
        return false;
    };
    colors.set(target, value);
    true
}

/// `NiTransformController` / `NiKeyframeController`: a `TimeController` that
/// drives one node's transform through an interpolator.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransformController {
    pub time: TimeController,
    pub interpolator: BlockRef,
}

/// `NiMaterialColorController`, selecting one `NiMaterialProperty` color.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MaterialColorController {
    pub time: TimeController,
    pub interpolator: BlockRef,
    pub target_color: MaterialColorTarget,
}

/// `NiPoint3Interpolator`, which supplies a material color track.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point3Interpolator {
    pub value: Vec3,
    pub data: BlockRef,
}

pub(crate) fn read_time_controller(s: &mut Stream<'_>) -> Result<TimeController> {
    Ok(TimeController {
        next_controller: s.block_ref()?,
        flags: ControllerFlags::from_bits(s.u16()?),
        frequency: s.f32()?,
        phase: s.f32()?,
        start_time: s.f32()?,
        stop_time: s.f32()?,
        target: s.block_ref()?,
    })
}

pub(crate) fn read_transform_controller(s: &mut Stream<'_>) -> Result<TransformController> {
    Ok(TransformController {
        time: read_time_controller(s)?,
        interpolator: s.block_ref()?,
    })
}

pub(crate) fn read_material_color_controller(
    s: &mut Stream<'_>,
    block: usize,
) -> Result<MaterialColorController> {
    let time = read_time_controller(s)?;
    let interpolator = s.block_ref()?;
    let target_color = match s.u16()? {
        0 => MaterialColorTarget::Ambient,
        1 => MaterialColorTarget::Diffuse,
        2 => MaterialColorTarget::Specular,
        3 => MaterialColorTarget::SelfIllumination,
        value => {
            return Err(Error::InvalidData {
                block,
                reason: format!("unknown material color target {value}"),
            });
        }
    };
    Ok(MaterialColorController {
        time,
        interpolator,
        target_color,
    })
}

pub(crate) fn read_point3_interpolator(s: &mut Stream<'_>) -> Result<Point3Interpolator> {
    Ok(Point3Interpolator {
        value: s.vec3()?,
        data: s.block_ref()?,
    })
}

/// Map a game-time value (seconds) to the controller's local track time,
/// applying frequency and phase and then extrapolating past
/// `[start_time, stop_time]` per the cycle type. Grounded on OpenMW's
/// `ControllerFunction::calculate()`, `components/nifosg/controller.cpp`
/// lines 20-71. Invalid clocks, or durations and frequencies at most one
/// microsecond or one millionth, hold the finite start pose (zero if the
/// start is invalid). Valid cycles use a closed form independent of elapsed
/// time. `CYCLE_CLAMP` holds the edge pose rather than resetting.
pub fn evaluate_time(time: &TimeController, game_time: f32) -> f32 {
    let Some(t) = controller_clock(time, game_time) else {
        return if time.start_time.is_finite() {
            time.start_time
        } else {
            0.0
        };
    };
    let start = f64::from(time.start_time);
    let stop = f64::from(time.stop_time);
    let span = stop - start;
    let value = if t >= start && t <= stop {
        t
    } else {
        match time.flags.cycle_type {
            CycleType::Clamp => t.clamp(start, stop),
            CycleType::Loop => start + (t - start).rem_euclid(span),
            CycleType::Reverse => {
                let remainder = (t - start).rem_euclid(2.0 * span);
                if remainder <= span {
                    start + remainder
                } else {
                    stop - (remainder - span)
                }
            }
        }
    };
    if time.flags.play_backwards {
        (stop - (value - start)) as f32
    } else {
        value as f32
    }
}

pub(crate) fn controller_clock(time: &TimeController, game_time: f32) -> Option<f64> {
    const MIN_TIMING: f32 = 1e-6;
    let span = time.stop_time - time.start_time;
    if !time.start_time.is_finite()
        || !time.stop_time.is_finite()
        || !span.is_finite()
        || span <= MIN_TIMING
        || !time.frequency.is_finite()
        || time.frequency <= MIN_TIMING
        || !time.phase.is_finite()
        || !game_time.is_finite()
    {
        return None;
    }
    // CONTEXT: f64 keeps finite f32 clock inputs from overflowing before wrapping.
    Some(f64::from(time.frequency) * f64::from(game_time) + f64::from(time.phase))
}

/// `KeyType`, `nif.xml` lines 399-406. Storage is `uint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    #[default]
    Linear,
    Quadratic,
    Tbc,
    /// Rotation keys only: three independent per-axis float tracks instead
    /// of one quaternion track.
    XyzRotation,
    Constant,
}

impl Interpolation {
    fn from_u32(v: u32, block: usize) -> Result<Interpolation> {
        match v {
            1 => Ok(Interpolation::Linear),
            2 => Ok(Interpolation::Quadratic),
            3 => Ok(Interpolation::Tbc),
            4 => Ok(Interpolation::XyzRotation),
            5 => Ok(Interpolation::Constant),
            other => Err(Error::InvalidData {
                block,
                reason: format!("unknown key type {other}"),
            }),
        }
    }
}

/// Tension, bias, continuity for a TCB key, `nif.xml` lines 1991-1996. File
/// order is t, b, c, which this struct keeps as its field order and name.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tbc {
    pub tension: f32,
    pub bias: f32,
    pub continuity: f32,
}

/// One key on a float or vector track, `Key<T>` in `nif.xml` lines 1998-2005.
/// Tangents are only meaningful for `Interpolation::Quadratic`, TBC only for
/// `Interpolation::Tbc`, both default to zero otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Key<T> {
    pub time: f32,
    pub value: T,
    pub in_tan: T,
    pub out_tan: T,
    pub tbc: Tbc,
}

/// One key on a quaternion track, `QuatKey<T>` in `nif.xml` lines 2014-2020.
/// Never carries tangents, per nif.xml's own note on the struct.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QuatKey {
    pub time: f32,
    pub value: Quat,
    pub tbc: Tbc,
}

/// `KeyGroup<T>`, `nif.xml` lines 2007-2012: a count, an interpolation mode
/// shared by every key in the group, and the keys themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyGroup<T> {
    pub interpolation: Interpolation,
    pub keys: Vec<Key<T>>,
}

impl<T> Default for KeyGroup<T> {
    fn default() -> Self {
        KeyGroup {
            interpolation: Interpolation::default(),
            keys: Vec::new(),
        }
    }
}

/// Values a key track can hold. `f32` and `Vec3` are the only two Skyrim SE
/// transform controllers use (float channels for XYZ Euler rotation and
/// path percent, `Vec3` for translation, scale reuses the float reader
/// directly since `KeyGroup<f32>` already covers it).
pub(crate) trait Numeric: Copy + Default {
    fn add(self, o: Self) -> Self;
    fn sub(self, o: Self) -> Self;
    fn scale(self, s: f32) -> Self;
}

impl Numeric for f32 {
    fn add(self, o: f32) -> f32 {
        self + o
    }
    fn sub(self, o: f32) -> f32 {
        self - o
    }
    fn scale(self, s: f32) -> f32 {
        self * s
    }
}

impl Numeric for Vec3 {
    fn add(self, o: Vec3) -> Vec3 {
        self + o
    }
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn scale(self, s: f32) -> Vec3 {
        Vec3::scale(self, s)
    }
}

// `read_value` is tied to the same data lifetime `'a` as `s`: a `Stream`
// method taken as a bare fn item (`Stream::f32`, `Stream::vec3`) is only
// generic over its own `&mut self` borrow, not over `Stream`'s own data
// lifetime, so the fn pointer parameter cannot be higher-ranked over `'a`.
fn read_key<'a, T: Numeric>(
    s: &mut Stream<'a>,
    interpolation: Interpolation,
    read_value: fn(&mut Stream<'a>) -> Result<T>,
) -> Result<Key<T>> {
    let time = s.f32()?;
    let value = read_value(s)?;
    let (in_tan, out_tan) = match interpolation {
        // File order is Forward then Backward. OpenMW's `readQuadratic`
        // (`components/nif/data.hpp`) maps file-Forward to its in-tangent
        // and file-Backward to its out-tangent, which this mirrors even
        // though the names read backwards.
        Interpolation::Quadratic => {
            let forward = read_value(s)?;
            let backward = read_value(s)?;
            (forward, backward)
        }
        _ => (T::default(), T::default()),
    };
    let tbc = match interpolation {
        Interpolation::Tbc => Tbc {
            tension: s.f32()?,
            bias: s.f32()?,
            continuity: s.f32()?,
        },
        _ => Tbc::default(),
    };
    Ok(Key {
        time,
        value,
        in_tan,
        out_tan,
        tbc,
    })
}

fn read_key_group<'a, T: Numeric>(
    s: &mut Stream<'a>,
    block: usize,
    read_value: fn(&mut Stream<'a>) -> Result<T>,
) -> Result<KeyGroup<T>> {
    let raw_n = s.u32()?;
    let n = s.count(raw_n, 4)?;
    if n == 0 {
        return Ok(KeyGroup::default());
    }
    let interpolation = Interpolation::from_u32(s.u32()?, block)?;
    let keys = (0..n)
        .map(|_| read_key(s, interpolation, read_value))
        .collect::<Result<Vec<_>>>()?;
    Ok(KeyGroup {
        interpolation,
        keys,
    })
}

fn read_quat_key(s: &mut Stream<'_>, interpolation: Interpolation) -> Result<QuatKey> {
    Ok(QuatKey {
        time: s.f32()?,
        value: s.quat()?,
        tbc: match interpolation {
            Interpolation::Tbc => Tbc {
                tension: s.f32()?,
                bias: s.f32()?,
                continuity: s.f32()?,
            },
            _ => Tbc::default(),
        },
    })
}

/// A transform data block's rotation channel: either one quaternion track,
/// or three independent Euler angle tracks (`Rotation Type` `XYZ_ROTATION`),
/// or nothing at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RotationKeys {
    #[default]
    None,
    Quaternion {
        interpolation: Interpolation,
        keys: Vec<QuatKey>,
    },
    Euler {
        x: KeyGroup<f32>,
        y: KeyGroup<f32>,
        z: KeyGroup<f32>,
    },
}

/// `NiTransformData` / `NiKeyframeData`, `nif.xml` lines 4327-4337 (Order,
/// the pre-10.1 rotation key data, is not present at our file version).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransformData {
    pub rotations: RotationKeys,
    pub translations: KeyGroup<Vec3>,
    pub scales: KeyGroup<f32>,
}

pub(crate) fn read_transform_data(s: &mut Stream<'_>, block: usize) -> Result<TransformData> {
    let raw_rot_n = s.u32()?;
    let rot_n = s.count(raw_rot_n, 4)?;
    let rotations = if rot_n == 0 {
        RotationKeys::None
    } else {
        let rotation_type = Interpolation::from_u32(s.u32()?, block)?;
        if rotation_type == Interpolation::XyzRotation {
            RotationKeys::Euler {
                x: read_key_group(s, block, Stream::f32)?,
                y: read_key_group(s, block, Stream::f32)?,
                z: read_key_group(s, block, Stream::f32)?,
            }
        } else {
            let keys = (0..rot_n)
                .map(|_| read_quat_key(s, rotation_type))
                .collect::<Result<Vec<_>>>()?;
            RotationKeys::Quaternion {
                interpolation: rotation_type,
                keys,
            }
        }
    };
    let translations = read_key_group(s, block, Stream::vec3)?;
    let scales = read_key_group(s, block, Stream::f32)?;
    Ok(TransformData {
        rotations,
        translations,
        scales,
    })
}

/// `NiPosData`, `nif.xml` lines 4712-4715: a thin wrapper with no field of
/// its own beyond the key group.
pub(crate) fn read_pos_data(s: &mut Stream<'_>, block: usize) -> Result<KeyGroup<Vec3>> {
    read_key_group(s, block, Stream::vec3)
}

/// `NiFloatData`, `nif.xml` lines 4259-4262: same thin-wrapper shape.
pub(crate) fn read_float_data(s: &mut Stream<'_>, block: usize) -> Result<KeyGroup<f32>> {
    read_key_group(s, block, Stream::f32)
}

/// `NiQuatTransform`, `nif.xml` lines 2199-2204 (`TRS Valid` is only present
/// until 10.1.0.109, not at our file version).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuatTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: f32,
}

impl Default for QuatTransform {
    fn default() -> Self {
        QuatTransform {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: 1.0,
        }
    }
}

/// `NiTransformInterpolator`, `nif.xml` lines 3228-3279: the default
/// transform plus a `NiTransformData` ref, which is `BlockRef::NONE` for a
/// controller whose track never leaves this default pose.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransformInterpolator {
    pub default_value: QuatTransform,
    pub data: BlockRef,
}

pub(crate) fn read_transform_interpolator(s: &mut Stream<'_>) -> Result<TransformInterpolator> {
    Ok(TransformInterpolator {
        default_value: QuatTransform {
            translation: s.vec3()?,
            rotation: s.quat()?,
            scale: s.f32()?,
        },
        data: s.block_ref()?,
    })
}

/// `PathFlags`, `nif.xml` lines 3260-3268.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathFlags {
    pub cv_data_needs_update: bool,
    pub curve_type_open: bool,
    pub allow_flip: bool,
    pub bank: bool,
    pub constant_velocity: bool,
    pub follow: bool,
    pub flip: bool,
}

impl PathFlags {
    fn from_bits(bits: u16) -> PathFlags {
        PathFlags {
            cv_data_needs_update: bits & 0x1 != 0,
            curve_type_open: bits & 0x2 != 0,
            allow_flip: bits & 0x4 != 0,
            bank: bits & 0x8 != 0,
            constant_velocity: bits & 0x10 != 0,
            follow: bits & 0x20 != 0,
            flip: bits & 0x40 != 0,
        }
    }
}

/// `NiPathInterpolator`, `nif.xml` lines 3270-3279: follows a `NiPosData`
/// curve with a `NiFloatData` percent-along-curve track. Parsed so the
/// block survives round trips, but curve evaluation is not implemented yet
/// (see `docs/nif.md`): no sampled Skyrim SE static in the corpus was found
/// to use it, `NiTransformController` covers the animated statics seen so
/// far.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PathInterpolator {
    pub flags: PathFlags,
    pub bank_dir: i32,
    pub max_bank_angle: f32,
    pub smoothing: f32,
    pub follow_axis: i16,
    pub path_data: BlockRef,
    pub percent_data: BlockRef,
}

pub(crate) fn read_path_interpolator(s: &mut Stream<'_>) -> Result<PathInterpolator> {
    Ok(PathInterpolator {
        flags: PathFlags::from_bits(s.u16()?),
        bank_dir: s.i32()?,
        max_bank_angle: s.f32()?,
        smoothing: s.f32()?,
        follow_axis: s.u16()? as i16,
        path_data: s.block_ref()?,
        percent_data: s.block_ref()?,
    })
}

/// Find the two keys bracketing `t`, clamping to the first or last key when
/// `t` falls outside the track, and the fraction between them. Mirrors
/// OpenMW's `KeyMapT::interpKey` search, `components/nif/data.hpp`.
fn bracket(times: &[f32], t: f32) -> (usize, usize, f32) {
    if times.len() <= 1 {
        return (0, 0, 0.0);
    }
    if t <= times[0] {
        return (0, 0, 0.0);
    }
    if t >= times[times.len() - 1] {
        let last = times.len() - 1;
        return (last, last, 0.0);
    }
    for i in 0..times.len() - 1 {
        if t >= times[i] && t <= times[i + 1] {
            let span = times[i + 1] - times[i];
            let frac = if span > 0.0 {
                (t - times[i]) / span
            } else {
                0.0
            };
            return (i, i + 1, frac);
        }
    }
    (times.len() - 1, times.len() - 1, 0.0)
}

/// Cubic Hermite blend of two keys' values and tangents, grounded on
/// OpenMW's `ValueInterpolator::interpolate`.
fn hermite<T: Numeric>(a: &Key<T>, b: &Key<T>, frac: f32) -> T {
    let t = frac;
    let t2 = t * t;
    let t3 = t2 * t;
    let b1 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let b2 = -2.0 * t3 + 3.0 * t2;
    let b3 = t3 - 2.0 * t2 + t;
    let b4 = t3 - t2;
    a.value
        .scale(b1)
        .add(b.value.scale(b2))
        .add(a.out_tan.scale(b3))
        .add(b.in_tan.scale(b4))
}

/// Evaluate a float or vector key track at local track time `t`. `None`
/// when the track carries no keys, so a caller can fall back to a default
/// value instead of the track's zero value.
pub(crate) fn evaluate_track<T: Numeric>(group: &KeyGroup<T>, t: f32) -> Option<T> {
    if group.keys.is_empty() {
        return None;
    }
    if group.keys.len() == 1 {
        return Some(group.keys[0].value);
    }
    let times: Vec<f32> = group.keys.iter().map(|k| k.time).collect();
    let (i, j, frac) = bracket(&times, t);
    if i == j {
        return Some(group.keys[i].value);
    }
    let a = &group.keys[i];
    let b = &group.keys[j];
    Some(match group.interpolation {
        // `nif.xml`'s `CONST_KEY` is a step function, and OpenMW's
        // `interpolate()` (`components/nifosg/controller.hpp`) implements
        // the step as `fraction > 0.5f ? b.mValue : a.mValue`: past the
        // midpoint between the two bracketing keys, snap to the later key.
        Interpolation::Constant => {
            if frac > 0.5 {
                b.value
            } else {
                a.value
            }
        }
        Interpolation::Quadratic => hermite(a, b, frac),
        Interpolation::Tbc => {
            let mut a = *a;
            let mut b = *b;
            a.out_tan = tbc_tangent(&group.keys, i, true);
            b.in_tan = tbc_tangent(&group.keys, j, false);
            hermite(&a, &b, frac)
        }
        Interpolation::Linear | Interpolation::XyzRotation => {
            a.value.add(b.value.sub(a.value).scale(frac))
        }
    })
}

fn tbc_tangent<T: Numeric>(keys: &[Key<T>], index: usize, outgoing: bool) -> T {
    let key = keys[index];
    let t = 1.0 - key.tbc.tension;
    let c = key.tbc.continuity;
    let b = key.tbc.bias;
    let (left, right) = if outgoing {
        (t * (1.0 + c) * (1.0 + b), t * (1.0 - c) * (1.0 - b))
    } else {
        (t * (1.0 - c) * (1.0 + b), t * (1.0 + c) * (1.0 - b))
    };
    if index == 0 {
        return keys[1]
            .value
            .sub(key.value)
            .scale(f32::midpoint(left, right));
    }
    if index + 1 == keys.len() {
        return key
            .value
            .sub(keys[index - 1].value)
            .scale(f32::midpoint(left, right));
    }
    let previous = keys[index - 1];
    let next = keys[index + 1];
    let span = next.time - previous.time;
    if span <= 0.0 {
        return T::default();
    }
    let weight = if outgoing {
        next.time - key.time
    } else {
        key.time - previous.time
    } / span;
    key.value
        .sub(previous.value)
        .scale(left)
        .add(next.value.sub(key.value).scale(right))
        .scale(weight)
}

/// Evaluate a quaternion key track. Always slerps between the bracketing
/// keys regardless of their interpolation mode, except `Constant` which
/// steps to the nearer key past the midpoint, matching OpenMW's rotation
/// path (`components/nifosg/controller.hpp`'s `osg::Quat interpolate()`
/// overload, `fraction > 0.5f ? b.mValue : a.mValue`; quadratic and TBC
/// quaternion tangents are not implemented there either).
fn evaluate_rotation(keys: &[QuatKey], interpolation: Interpolation, t: f32) -> Option<Quat> {
    if keys.is_empty() {
        return None;
    }
    if keys.len() == 1 {
        return Some(keys[0].value);
    }
    let times: Vec<f32> = keys.iter().map(|k| k.time).collect();
    let (i, j, frac) = bracket(&times, t);
    if i == j {
        return Some(keys[i].value);
    }
    if interpolation == Interpolation::Constant {
        return Some(if frac > 0.5 {
            keys[j].value
        } else {
            keys[i].value
        });
    }
    Some(keys[i].value.slerp(keys[j].value, frac))
}

/// Rotation matrix for a right-handed rotation of `angle` radians about the
/// X (0), Y (1) or Z (2) axis.
fn axis_rotation(axis: u8, angle: f32) -> Mat33 {
    let (sin, cos) = angle.sin_cos();
    match axis {
        0 => Mat33 {
            rows: [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]],
        },
        1 => Mat33 {
            rows: [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]],
        },
        _ => Mat33 {
            rows: [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
        },
    }
}

/// Compose three independently evaluated Euler angle tracks into a rotation
/// matrix. `nif.xml` gives no field naming an axis order for
/// `XYZ_ROTATION_KEY` tracks (searched, no match), so this follows the file
/// field order (X, then Y, then Z) and OpenMW's default case in
/// `KeyframeController::getXYZRotation` (`components/nifosg/controller.cpp`
/// lines 136-170), which composes `xr * yr * zr` when no other order is
/// known. Whether any Skyrim SE asset needs a different order is unverified,
/// see `docs/nif.md`.
fn evaluate_euler(x: &KeyGroup<f32>, y: &KeyGroup<f32>, z: &KeyGroup<f32>, t: f32) -> Mat33 {
    let xr = axis_rotation(0, evaluate_track(x, t).unwrap_or(0.0));
    let yr = axis_rotation(1, evaluate_track(y, t).unwrap_or(0.0));
    let zr = axis_rotation(2, evaluate_track(z, t).unwrap_or(0.0));
    xr.mul(&yr).mul(&zr)
}

/// Evaluate a `NiTransformController` at a game time in seconds: applies
/// frequency, phase and cycle mode, then blends whichever key tracks
/// `data` carries, falling back to the interpolator's own default value for
/// any channel the data block does not have (an empty `TransformData`, or
/// no data block at all, means "hold the interpolator's default pose",
/// matching OpenMW's per-channel presence checks in
/// `KeyframeController::getCurrentTransformation`).
pub fn evaluate_transform(
    time: &TimeController,
    interp: &TransformInterpolator,
    data: Option<&TransformData>,
    game_time: f32,
) -> Transform {
    let t = evaluate_time(time, game_time);
    let rotation = match data.map(|d| &d.rotations) {
        Some(RotationKeys::Quaternion {
            interpolation,
            keys,
        }) if !keys.is_empty() => evaluate_rotation(keys, *interpolation, t)
            .unwrap_or(interp.default_value.rotation)
            .to_mat33(),
        Some(RotationKeys::Euler { x, y, z })
            if !x.keys.is_empty() || !y.keys.is_empty() || !z.keys.is_empty() =>
        {
            evaluate_euler(x, y, z, t)
        }
        _ => interp.default_value.rotation.to_mat33(),
    };
    let translation = data
        .and_then(|d| evaluate_track(&d.translations, t))
        .unwrap_or(interp.default_value.translation);
    let scale = data
        .and_then(|d| evaluate_track(&d.scales, t))
        .unwrap_or(interp.default_value.scale);
    Transform {
        rotation,
        translation,
        scale,
    }
}

/// True when a `TimeController`'s flags mark it active, the state a host
/// should check before spending a frame evaluating it.
pub fn is_active(time: &TimeController) -> bool {
    time.flags.active
}
