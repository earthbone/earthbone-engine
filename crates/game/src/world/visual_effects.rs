// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient VisualEffect.Play and Stop requests over RFCT forms and placed
//! references. Active instances are exposed to hosts in stable id order.

use soe_esm::GlobalFormId;

use super::World;

/// Identity of one VisualEffect.Play call. Calls stack, even for the same
/// effect and target, until they expire or Stop removes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisualEffectId(pub u64);

/// One transient visual effect requested by Papyrus.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualEffectInstance {
    pub id: VisualEffectId,
    pub effect: GlobalFormId,
    pub target: GlobalFormId,
    pub remaining: Option<f64>,
    pub facing: Option<GlobalFormId>,
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
    /// Active VisualEffect requests in stable call order.
    pub fn visual_effect_instances(&self) -> impl Iterator<Item = &VisualEffectInstance> {
        self.visual_effects.values()
    }

    /// Start an RFCT visual effect on a placed reference.
    ///
    /// A negative duration means the effect runs until stopped. Repeated calls
    /// create distinct instances, as the CK contract specifies.
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
        if let Some(facing) = facing {
            if !self.is_placed_reference(facing) {
                return Err(VisualEffectError::Target(facing));
            }
        }
        if duration.is_nan() {
            return Err(VisualEffectError::Duration);
        }
        let id = VisualEffectId(self.next_visual_effect);
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
            },
        );
        self.next_visual_effect = next;
        Ok(id)
    }

    /// Stop every instance of this effect on the target and return the number
    /// removed. Other effects and targets remain active.
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

    fn is_placed_reference(&self, id: GlobalFormId) -> bool {
        self.refs.contains_key(&id)
            || self.record(id).is_some_and(|record| {
                !record.is_deleted() && matches!(record.tag.as_bytes(), b"REFR" | b"ACHR")
            })
    }

    fn is_effect_form(&self, id: GlobalFormId) -> bool {
        self.record(id)
            .is_some_and(|record| !record.is_deleted() && record.tag.as_bytes() == b"RFCT")
    }
}
