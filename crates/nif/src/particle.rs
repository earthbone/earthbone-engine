// SPDX-License-Identifier: GPL-3.0-or-later

//! SSE particle emitter timing from nifxml's `NiParticleSystem`,
//! `NiPSysEmitter`, and `NiPSysEmitterCtlr` fields. nifly's `Particles.cpp`
//! confirms the stream 100 field order. OpenMW's NIFOSG particle code uses
//! the authored birth rate and lifespan, but its renderer is OSG-specific.

use crate::blocks::Block;
use crate::controller::{evaluate_time, evaluate_track, TimeController};
use crate::error::{Error, Result};
use crate::stream::Stream;
use crate::types::{BlockRef, Transform};
use crate::NifFile;

/// A parsed emitter and the controller that supplies its births per second.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    pub name: String,
    pub system_name: String,
    pub transform: Transform,
    pub rate: Rate,
    pub lifetime: f32,
    pub lifetime_variation: f32,
    pub radius: f32,
    pub color: [f32; 4],
    pub capacity: usize,
}

/// Constant or keyed birth rate on a `NiPSysEmitterCtlr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    pub time: TimeController,
    pub value: f32,
    pub keys: Option<crate::controller::KeyGroup<f32>>,
    pub visibility: Visibility,
}

/// The emitter-active channel uses constant boolean keys in `NiBoolData`.
#[derive(Debug, Clone, PartialEq)]
pub struct Visibility {
    pub default: bool,
    pub keys: Vec<(f32, bool)>,
}

impl Visibility {
    fn at(&self, time: f32) -> bool {
        self.keys
            .iter()
            .rev()
            .find(|(key_time, _)| *key_time <= time)
            .map_or(self.default, |(_, value)| *value)
    }
}

impl Rate {
    /// Return births per second at elapsed scene time.
    pub fn at(&self, elapsed: f32) -> f32 {
        let t = evaluate_time(&self.time, elapsed);
        if !self.time.flags.active || !self.visibility.at(t) {
            return 0.0;
        }
        self.keys
            .as_ref()
            .and_then(|keys| evaluate_track(keys, t))
            .unwrap_or(self.value)
            .max(0.0)
    }
}

/// One live particle's timing and local position, in NIF units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub born_at: f32,
    pub lifetime: f32,
    pub position: [f32; 3],
}

/// Bounded CPU timing state. The host can upload `particles` to one MultiMesh.
#[derive(Debug, Clone)]
pub struct Timeline {
    emitter: Emitter,
    elapsed: f32,
    credit: f64,
    emitted: u64,
    particles: Vec<Particle>,
}

impl Timeline {
    /// Start an emitter at scene time zero.
    pub fn new(emitter: Emitter) -> Self {
        Self {
            emitter,
            elapsed: 0.0,
            credit: 0.0,
            emitted: 0,
            particles: Vec::new(),
        }
    }

    /// Advance births and deaths by a finite, nonnegative number of seconds.
    pub fn advance(&mut self, delta: f32) {
        if !delta.is_finite() || delta <= 0.0 {
            return;
        }
        let end = self.elapsed + delta;
        if !end.is_finite() {
            return;
        }
        let keyed = self
            .emitter
            .rate
            .keys
            .as_ref()
            .is_some_and(|keys| !keys.keys.is_empty())
            || !self.emitter.rate.visibility.keys.is_empty();
        let slices = if keyed {
            (delta * 60.0).ceil().clamp(1.0, 4096.0) as usize
        } else {
            1
        };
        let start = self.elapsed;
        for slice in 1..=slices {
            let next = if slice == slices {
                end
            } else {
                start + delta * (slice as f32 / slices as f32)
            };
            self.advance_slice(next);
        }
    }

    fn advance_slice(&mut self, end: f32) {
        let delta = end - self.elapsed;
        let rate = self.emitter.rate.at(self.elapsed + delta * 0.5);
        if rate > 0.0 {
            let old_credit = self.credit;
            self.credit += f64::from(rate) * f64::from(delta);
            let whole = (self.credit + 1e-8).floor();
            let births = whole.min(65_536.0) as usize;
            self.credit -= whole;
            let skipped = whole - births as f64;
            for index in 0..births {
                let born = self.elapsed
                    + ((skipped + index as f64 + 1.0 - old_credit) / f64::from(rate)) as f32;
                if self.emitter.rate.at(born) <= 0.0 {
                    continue;
                }
                self.particles.retain(|p| p.born_at + p.lifetime > born);
                if self.particles.len() < self.emitter.capacity {
                    let random = random_fraction(self.emitted);
                    self.particles.push(Particle {
                        born_at: born,
                        lifetime: (self.emitter.lifetime
                            + self.emitter.lifetime_variation * random)
                            .max(f32::EPSILON),
                        position: self.emitter.transform.translation.to_array(),
                    });
                    self.emitted += 1;
                }
            }
        }
        self.particles.retain(|p| p.born_at + p.lifetime > end);
        self.elapsed = end;
    }

    /// Number of particles currently alive.
    pub fn alive(&self) -> usize {
        self.particles.len()
    }

    /// Total particles emitted since creation.
    pub fn emitted(&self) -> u64 {
        self.emitted
    }

    /// Live particles in birth order.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// Authored emitter properties.
    pub fn emitter(&self) -> &Emitter {
        &self.emitter
    }
}

fn random_fraction(index: u64) -> f32 {
    let mut bits = index.wrapping_add(0x9E37_79B9_7F4A_7C15);
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((bits ^ (bits >> 31)) >> 40) as f32 / 16_777_216.0
}

struct System {
    name: String,
    transform: Transform,
    data: BlockRef,
    modifiers: Vec<BlockRef>,
}

fn raw<'a>(file: &'a NifFile, reference: BlockRef, kind: &str) -> Option<&'a [u8]> {
    match file.get(reference)? {
        Block::Unknown { type_name, data } if type_name == kind => Some(data),
        _ => None,
    }
}

fn system(file: &NifFile, bytes: &[u8]) -> Result<System> {
    let mut s = Stream::new(bytes);
    let name = s.string_index(&file.header.strings)?;
    let _extra = s.block_refs()?;
    let _controller = s.block_ref()?;
    let _flags = s.u32()?;
    let translation = s.vec3()?;
    let rotation = s.mat33()?;
    let scale = s.f32()?;
    let transform = Transform {
        rotation,
        translation,
        scale,
    };
    let _collision = s.block_ref()?;
    let _bound = s.bound()?;
    let _skin = s.block_ref()?;
    let _shader = s.block_ref()?;
    let _alpha = s.block_ref()?;
    let _vertex_desc = s.u64()?;
    for _ in 0..4 {
        s.u16()?;
    }
    let data = s.block_ref()?;
    let _world_space = s.bool()?;
    let modifiers = s.block_refs()?;
    Ok(System {
        name,
        transform,
        data,
        modifiers,
    })
}

struct Modifier {
    name: String,
    target: BlockRef,
    active: bool,
    lifetime: f32,
    lifetime_variation: f32,
    radius: f32,
    color: [f32; 4],
}

fn modifier(file: &NifFile, bytes: &[u8]) -> Result<Modifier> {
    let mut s = Stream::new(bytes);
    let name = s.string_index(&file.header.strings)?;
    let _order = s.u32()?;
    let target = s.block_ref()?;
    let active = s.bool()?;
    for _ in 0..6 {
        s.f32()?;
    }
    let color = s.color4()?;
    let radius = s.f32()?;
    let _radius_variation = s.f32()?;
    let lifetime = s.f32()?;
    let lifetime_variation = s.f32()?;
    Ok(Modifier {
        name,
        target,
        active,
        lifetime,
        lifetime_variation,
        radius,
        color,
    })
}

fn controller(
    file: &NifFile,
    bytes: &[u8],
) -> Result<(TimeController, BlockRef, String, BlockRef)> {
    let mut s = Stream::new(bytes);
    let time = crate::controller::read_time_controller(&mut s)?;
    let interpolator = s.block_ref()?;
    let modifier_name = s.string_index(&file.header.strings)?;
    let visible = s.block_ref()?;
    Ok((time, interpolator, modifier_name, visible))
}

fn visibility(file: &NifFile, reference: BlockRef) -> Result<Visibility> {
    let Some(bytes) = raw(file, reference, "NiBoolInterpolator") else {
        return Ok(Visibility {
            default: true,
            keys: Vec::new(),
        });
    };
    let mut s = Stream::new(bytes);
    let default = s.u8()? != 0;
    let data_ref = s.block_ref()?;
    let Some(bytes) = raw(file, data_ref, "NiBoolData") else {
        return Ok(Visibility {
            default,
            keys: Vec::new(),
        });
    };
    let mut s = Stream::new(bytes);
    let count = s.counted(5)?;
    let kind = if count > 0 { s.u32()? } else { 5 };
    if kind != 5 && kind != 1 {
        return Err(Error::InvalidData {
            block: data_ref.get().unwrap_or(0),
            reason: format!("unsupported boolean key interpolation {kind}"),
        });
    }
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        let time = s.f32()?;
        let value = s.u8()? != 0;
        if !time.is_finite() {
            return Err(Error::InvalidData {
                block: data_ref.get().unwrap_or(0),
                reason: "particle visibility key time must be finite".into(),
            });
        }
        keys.push((time, value));
    }
    Ok(Visibility { default, keys })
}

fn rate(file: &NifFile, system_ref: BlockRef, name: &str) -> Result<Option<Rate>> {
    for (index, block) in file.blocks.iter().enumerate() {
        let Block::Unknown { type_name, data } = block else {
            continue;
        };
        if type_name != "NiPSysEmitterCtlr" && type_name != "BSPSysMultiTargetEmitterCtlr" {
            continue;
        }
        let (time, interpolator, modifier_name, visibility_ref) = controller(file, data)?;
        if time.target != system_ref || modifier_name != name {
            continue;
        }
        if ![time.frequency, time.phase, time.start_time, time.stop_time]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(Error::InvalidData {
                block: index,
                reason: "particle controller time must be finite".into(),
            });
        }
        let Some(bytes) = raw(file, interpolator, "NiFloatInterpolator") else {
            continue;
        };
        let mut s = Stream::new(bytes);
        let value = s.f32()?;
        if !value.is_finite() {
            return Err(Error::InvalidData {
                block: index,
                reason: "particle birth rate must be finite".into(),
            });
        }
        let keys_ref = s.block_ref()?;
        let keys = match file.get(keys_ref) {
            Some(Block::FloatData(keys)) => Some(keys.clone()),
            _ => None,
        };
        if keys.as_ref().is_some_and(|group| {
            group
                .keys
                .iter()
                .any(|key| !key.time.is_finite() || !key.value.is_finite())
        }) {
            return Err(Error::InvalidData {
                block: keys_ref.get().unwrap_or(index),
                reason: "particle birth rate keys must be finite".into(),
            });
        }
        return Ok(Some(Rate {
            time,
            value,
            keys,
            visibility: visibility(file, visibility_ref)?,
        }));
    }
    Ok(None)
}

/// Extract SSE volume emitters and their authored timing from a parsed NIF.
/// Unsupported emitter geometries remain opaque NIF blocks.
pub fn emitters(file: &NifFile) -> Result<Vec<Emitter>> {
    if file.header.bs_version != crate::header::BS_SKYRIM_SE {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (index, block) in file.blocks.iter().enumerate() {
        let Block::Unknown { type_name, data } = block else {
            continue;
        };
        if type_name != "NiParticleSystem" && type_name != "NiMeshParticleSystem" {
            continue;
        }
        let sys = system(file, data)?;
        let capacity = if let Some(data) = raw(file, sys.data, "NiPSysData") {
            let mut s = Stream::new(data);
            let _group = s.i32()?;
            usize::from(s.u16()?).min(8192)
        } else {
            8192
        };
        for reference in sys.modifiers {
            let Some(Block::Unknown { type_name, data }) = file.get(reference) else {
                continue;
            };
            if ![
                "NiPSysBoxEmitter",
                "NiPSysSphereEmitter",
                "NiPSysCylinderEmitter",
            ]
            .contains(&type_name.as_str())
            {
                continue;
            }
            let source = modifier(file, data)?;
            if !source.active || source.target != BlockRef(index as i32) {
                continue;
            }
            if !source.lifetime.is_finite()
                || source.lifetime <= 0.0
                || !source.lifetime_variation.is_finite()
            {
                return Err(Error::InvalidData {
                    block: reference.get().unwrap_or(index),
                    reason: "particle lifetime must be finite and positive".into(),
                });
            }
            let Some(rate) = rate(file, BlockRef(index as i32), &source.name)? else {
                continue;
            };
            out.push(Emitter {
                name: source.name,
                system_name: sys.name.clone(),
                transform: sys.transform,
                rate,
                lifetime: source.lifetime,
                lifetime_variation: source.lifetime_variation,
                radius: source.radius,
                color: source.color,
                capacity,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Emitter, Rate, Timeline, Visibility};
    use crate::controller::{ControllerFlags, CycleType, TimeController};
    use crate::types::Transform;

    #[test]
    fn rnd_14_visibility_keys_switch_births_at_the_authored_time() {
        let visibility = Visibility {
            default: false,
            keys: vec![(0.5, true), (1.0, false)],
        };
        assert!(!visibility.at(0.25));
        assert!(visibility.at(0.5));
        assert!(visibility.at(0.75));
        assert!(!visibility.at(1.0));
    }

    #[test]
    fn rnd_14_keyed_visibility_changes_births_within_one_advance() {
        let emitter = Emitter {
            name: "RND14Keyed".into(),
            system_name: "RND14System".into(),
            transform: Transform::IDENTITY,
            rate: Rate {
                time: TimeController {
                    flags: ControllerFlags {
                        active: true,
                        cycle_type: CycleType::Clamp,
                        ..Default::default()
                    },
                    frequency: 1.0,
                    stop_time: 10.0,
                    ..Default::default()
                },
                value: 4.0,
                keys: None,
                visibility: Visibility {
                    default: false,
                    keys: vec![(0.5, true)],
                },
            },
            lifetime: 10.0,
            lifetime_variation: 0.0,
            radius: 1.0,
            color: [1.0; 4],
            capacity: 16,
        };
        let mut timeline = Timeline::new(emitter);
        timeline.advance(1.0);
        assert_eq!(timeline.emitted(), 2);
        assert_eq!(timeline.alive(), 2);
    }
}
