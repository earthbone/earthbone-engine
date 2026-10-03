// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient VisualEffect.Play and Stop requests over RFCT forms and placed
//! references. Active instances are exposed to hosts in stable call order.

use soe_esm::GlobalFormId;

use super::{le_u32, World, T};

/// The authored forms and flags in an RFCT record's DATA subrecord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualEffectData {
    pub art: Option<GlobalFormId>,
    pub shader: Option<GlobalFormId>,
    pub flags: u32,
}

/// One active VisualEffect.Play call. Calls stack until they expire or Stop
/// removes them.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualEffectInstance {
    pub id: VisualEffectId,
    pub effect: GlobalFormId,
    pub target: GlobalFormId,
    pub remaining: Option<f64>,
    pub facing: Option<GlobalFormId>,
    /// RFCT DATA flags carried to host presentation.
    pub flags: u32,
}

/// Identity of one VisualEffect.Play call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisualEffectId(pub u64);

/// One authored RFCT attachment used by the direct runtime API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualEffectAttachment {
    pub effect: GlobalFormId,
    pub target: GlobalFormId,
    pub art: Option<GlobalFormId>,
    pub shader: Option<GlobalFormId>,
    pub flags: u32,
}

/// A rejected visual effect request.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VisualEffectError {
    #[error("visual effect form {0:?} is missing or is not RFCT")]
    Effect(GlobalFormId),
    #[error("visual effect target {0:?} is missing or is not a placed reference")]
    Target(GlobalFormId),
    #[error("visual effect duration must not be NaN")]
    Duration,
    #[error("visual effect identity exhausted")]
    Identity,
    #[error("visual effect tick duration must be finite and nonnegative")]
    Tick,
}

impl World {
    /// Read the ARTO, EFSH and flags fields from a winning RFCT record.
    pub fn visual_effect_data(&self, id: GlobalFormId) -> Option<VisualEffectData> {
        let record = self.record(id)?;
        if record.tag != T(b"RFCT") || record.is_deleted() {
            return None;
        }
        let data = record.find(T(b"DATA"))?;
        if data.len() != 12 {
            return None;
        }
        let resolve = |offset| {
            le_u32(&data[offset..])
                .filter(|raw| *raw != 0)
                .and_then(|raw| self.resolve_from(id, raw))
        };
        Some(VisualEffectData {
            art: resolve(0),
            shader: resolve(4),
            flags: le_u32(&data[8..])?,
        })
    }

    /// Active VisualEffect requests in stable call order.
    pub fn visual_effect_instances(&self) -> impl Iterator<Item = &VisualEffectInstance> {
        self.visual_effects.values()
    }

    /// Start an RFCT visual effect on a placed reference.
    ///
    /// A negative duration means the effect runs until stopped. Repeated calls
    /// create distinct instances.
    pub fn play_visual_effect(
        &mut self,
        effect: GlobalFormId,
        target: GlobalFormId,
        duration: f64,
        facing: Option<GlobalFormId>,
    ) -> Result<VisualEffectId, VisualEffectError> {
        if !self.is_effect_form(effect) {
            return Err(VisualEffectError::Effect(effect));
        }
        if !self.is_placed_reference(target) {
            return Err(VisualEffectError::Target(target));
        }
        if let Some(form) = facing {
            if !self.is_placed_reference(form) {
                return Err(VisualEffectError::Target(form));
            }
        }
        if duration.is_nan() {
            return Err(VisualEffectError::Duration);
        }
        let id = VisualEffectId(self.next_visual_effect);
        let data = self
            .visual_effect_data(effect)
            .ok_or(VisualEffectError::Effect(effect))?;
        let next = self
            .next_visual_effect
            .checked_add(1)
            .ok_or(VisualEffectError::Identity)?;
        self.visual_effects.insert(
            id,
            VisualEffectInstance {
                id,
                effect,
                target,
                remaining: (duration >= 0.0 && duration.is_finite()).then_some(duration),
                facing,
                flags: data.flags,
            },
        );
        self.attach_visual_effect(effect, target);
        self.next_visual_effect = next;
        Ok(id)
    }

    /// Stop every instance of an effect on a target.
    pub fn stop_visual_effect(
        &mut self,
        effect: GlobalFormId,
        target: GlobalFormId,
    ) -> Result<usize, VisualEffectError> {
        if !self.is_effect_form(effect) {
            return Err(VisualEffectError::Effect(effect));
        }
        if !self.is_placed_reference(target) {
            return Err(VisualEffectError::Target(target));
        }
        let ids: Vec<_> = self
            .visual_effects
            .iter()
            .filter_map(|(&id, instance)| {
                (instance.effect == effect && instance.target == target).then_some(id)
            })
            .collect();
        for id in &ids {
            self.visual_effects.remove(id);
        }
        self.detach_visual_effect(effect, target);
        Ok(ids.len())
    }

    /// Advance finite VisualEffect.Play instances and remove expired ones.
    pub fn tick_visual_effects(&mut self, seconds: f64) -> Result<(), VisualEffectError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(VisualEffectError::Tick);
        }
        let mut expired = Vec::new();
        for (&id, instance) in &mut self.visual_effects {
            let Some(remaining) = &mut instance.remaining else {
                continue;
            };
            *remaining = (*remaining - seconds).max(0.0);
            if *remaining == 0.0 {
                expired.push(id);
            }
        }
        for id in expired {
            self.visual_effects.remove(&id);
        }
        Ok(())
    }

    /// Attach authored RFCT data for hosts that need resolved art and shader
    /// links. Replaying a pair replaces its prior attachment.
    pub fn attach_visual_effect(&mut self, effect: GlobalFormId, target: GlobalFormId) -> bool {
        let Some(data) = self.visual_effect_data(effect) else {
            return false;
        };
        if !self.is_placed_reference(target) {
            return false;
        }
        self.visual_effect_attachments.insert(
            (effect, target),
            VisualEffectAttachment {
                effect,
                target,
                art: data.art,
                shader: data.shader,
                flags: data.flags,
            },
        );
        true
    }

    /// Stop one authored RFCT attachment.
    pub fn detach_visual_effect(&mut self, effect: GlobalFormId, target: GlobalFormId) -> bool {
        self.visual_effect_attachments
            .remove(&(effect, target))
            .is_some()
    }

    /// Active authored attachments in deterministic form-id order.
    pub fn visual_effects(&self) -> impl Iterator<Item = &VisualEffectAttachment> {
        self.visual_effect_attachments.values()
    }

    fn is_placed_reference(&self, id: GlobalFormId) -> bool {
        self.refs.contains_key(&id)
            || self.record(id).is_some_and(|record| {
                !record.is_deleted() && matches!(record.tag.as_bytes(), b"REFR" | b"ACHR")
            })
    }

    fn is_effect_form(&self, id: GlobalFormId) -> bool {
        self.visual_effect_data(id).is_some()
    }
}
