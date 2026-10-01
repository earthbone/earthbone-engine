// SPDX-License-Identifier: GPL-3.0-or-later

//! Shader particle geometry application state from SPGD records and the
//! Papyrus ShaderParticleGeometry script.

use std::collections::BTreeMap;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::VmError;

use super::{World, T};

/// Particle parameters authored by an SPGD record.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderParticleGeometry {
    /// Source form.
    pub form: GlobalFormId,
    /// Gravity velocity from `DATA`.
    pub gravity_velocity: f32,
    /// Rotation velocity from `DATA`.
    pub rotation_velocity: f32,
    /// Particle width from `DATA`.
    pub particle_size_x: f32,
    /// Particle height from `DATA`.
    pub particle_size_y: f32,
    /// Minimum center offset from `DATA`.
    pub center_offset_min: f32,
    /// Maximum center offset from `DATA`.
    pub center_offset_max: f32,
    /// Initial rotation range from `DATA`.
    pub initial_rotation_range: f32,
    /// Horizontal subtexture count.
    pub subtextures_x: u32,
    /// Vertical subtexture count.
    pub subtextures_y: u32,
    /// Rain or snow emitter type.
    pub particle_type: u32,
    /// Emitter box size in game units.
    pub box_size: u32,
    /// Authored particle density.
    pub particle_density: f32,
    /// Optional texture path from `ICON`.
    pub texture: Option<String>,
}

/// An active SPGD effect and its current host-facing opacity.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderParticleLayer {
    /// Authored geometry definition.
    pub geometry: ShaderParticleGeometry,
    /// Current fade strength, from 0 to 1.
    pub strength: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum Fade {
    In {
        elapsed: f64,
        duration: f64,
    },
    Out {
        elapsed: f64,
        duration: f64,
        from: f32,
    },
    Settled,
}

#[derive(Debug, Clone, PartialEq)]
struct Application {
    geometry: ShaderParticleGeometry,
    fade: Fade,
    strength: f32,
}

/// Transient active particle geometry effects.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ShaderParticleState {
    active: BTreeMap<GlobalFormId, Application>,
}

impl World {
    fn shader_particle_geometry(
        &self,
        form: GlobalFormId,
    ) -> Result<ShaderParticleGeometry, VmError> {
        let record = self
            .record(form)
            .filter(|record| record.tag == T(b"SPGD") && record.flags & 0x20 == 0)
            .ok_or_else(|| VmError::State("missing ShaderParticleGeometry SPGD form".into()))?;
        let data = record
            .find(Tag::new(b"DATA"))
            .ok_or_else(|| VmError::State("SPGD record has no DATA".into()))?;
        if data.len() != 40 && data.len() != 48 {
            return Err(VmError::State("invalid SPGD DATA size".into()));
        }
        let mut bytes = data;
        let gravity_velocity = read_f32(&mut bytes)?;
        let rotation_velocity = read_f32(&mut bytes)?;
        let particle_size_x = read_f32(&mut bytes)?;
        let particle_size_y = read_f32(&mut bytes)?;
        let center_offset_min = read_f32(&mut bytes)?;
        let center_offset_max = read_f32(&mut bytes)?;
        let initial_rotation_range = read_f32(&mut bytes)?;
        let subtextures_x = read_u32(&mut bytes)?;
        let subtextures_y = read_u32(&mut bytes)?;
        let particle_type = read_u32(&mut bytes)?;
        let (box_size, particle_density) = if bytes.is_empty() {
            (4096, 1.0)
        } else {
            (read_u32(&mut bytes)?, read_f32(&mut bytes)?)
        };
        let finite = [
            gravity_velocity,
            rotation_velocity,
            particle_size_x,
            particle_size_y,
            center_offset_min,
            center_offset_max,
            initial_rotation_range,
            particle_density,
        ]
        .into_iter()
        .all(f32::is_finite);
        if !finite
            || particle_type > 1
            || subtextures_x == 0
            || subtextures_y == 0
            || box_size == 0
            || particle_density < 0.0
            || center_offset_min > center_offset_max
        {
            return Err(VmError::State("invalid SPGD particle parameters".into()));
        }
        let texture = record
            .find(Tag::new(b"ICON"))
            .map(|path| {
                String::from_utf8_lossy(path)
                    .trim_end_matches('\0')
                    .to_owned()
            })
            .filter(|path| !path.is_empty());
        Ok(ShaderParticleGeometry {
            form,
            gravity_velocity,
            rotation_velocity,
            particle_size_x,
            particle_size_y,
            center_offset_min,
            center_offset_max,
            initial_rotation_range,
            subtextures_x,
            subtextures_y,
            particle_type,
            box_size,
            particle_density,
            texture,
        })
    }

    /// Start or restart one SPGD effect, fading it in over `fade_seconds`.
    ///
    /// # Errors
    /// Returns an error if the form is absent, deleted, malformed, or the
    /// requested fade is not finite and nonnegative.
    pub fn apply_shader_particle_geometry(
        &mut self,
        form: GlobalFormId,
        fade_seconds: f32,
    ) -> Result<(), VmError> {
        let geometry = self.shader_particle_geometry(form)?;
        if !fade_seconds.is_finite() || fade_seconds < 0.0 {
            return Err(VmError::State("invalid SPGD fade-in time".into()));
        }
        let duration = f64::from(fade_seconds);
        let strength = if duration == 0.0 { 1.0 } else { 0.0 };
        self.shader_particles.active.insert(
            form,
            Application {
                geometry,
                fade: if duration == 0.0 {
                    Fade::Settled
                } else {
                    Fade::In {
                        elapsed: 0.0,
                        duration,
                    }
                },
                strength,
            },
        );
        Ok(())
    }

    /// Stop an SPGD effect immediately or fade it out over `fade_seconds`.
    ///
    /// # Errors
    /// Returns an error if the requested fade is not finite and nonnegative.
    pub fn remove_shader_particle_geometry(
        &mut self,
        form: GlobalFormId,
        fade_seconds: f32,
    ) -> Result<(), VmError> {
        if !fade_seconds.is_finite() || fade_seconds < 0.0 {
            return Err(VmError::State("invalid SPGD fade-out time".into()));
        }
        let duration = f64::from(fade_seconds);
        if duration == 0.0 {
            self.shader_particles.active.remove(&form);
        } else {
            let Some(application) = self.shader_particles.active.get_mut(&form) else {
                return Ok(());
            };
            application.fade = Fade::Out {
                elapsed: 0.0,
                duration,
                from: application.strength,
            };
        }
        Ok(())
    }

    /// Snapshot active SPGD effects in resolved form order for host rendering.
    pub fn shader_particle_layers(&self) -> Vec<ShaderParticleLayer> {
        self.shader_particles
            .active
            .values()
            .map(|application| ShaderParticleLayer {
                geometry: application.geometry.clone(),
                strength: application.strength,
            })
            .collect()
    }

    pub(crate) fn tick_shader_particles(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds < 0.0 {
            return;
        }
        let mut expired = Vec::new();
        for (form, application) in &mut self.shader_particles.active {
            match &mut application.fade {
                Fade::In { elapsed, duration } => {
                    *elapsed = (*elapsed + seconds).min(*duration);
                    application.strength = (*elapsed / *duration) as f32;
                    if *elapsed == *duration {
                        application.fade = Fade::Settled;
                    }
                }
                Fade::Out {
                    elapsed,
                    duration,
                    from,
                } => {
                    *elapsed = (*elapsed + seconds).min(*duration);
                    application.strength = *from * (1.0 - (*elapsed / *duration) as f32);
                    if *elapsed == *duration {
                        expired.push(*form);
                    }
                }
                Fade::Settled => {}
            }
        }
        for form in expired {
            self.shader_particles.active.remove(&form);
        }
    }
}

fn read_f32(bytes: &mut &[u8]) -> Result<f32, VmError> {
    let Some((head, tail)) = bytes.split_first_chunk::<4>() else {
        return Err(VmError::State("truncated SPGD DATA".into()));
    };
    *bytes = tail;
    Ok(f32::from_le_bytes(*head))
}

fn read_u32(bytes: &mut &[u8]) -> Result<u32, VmError> {
    let Some((head, tail)) = bytes.split_first_chunk::<4>() else {
        return Err(VmError::State("truncated SPGD DATA".into()));
    };
    *bytes = tail;
    Ok(u32::from_le_bytes(*head))
}
