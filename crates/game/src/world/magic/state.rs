// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW versions 5 and 37 store active applications after the world body.
//! Loaded actor values already contain recoverable modifiers. Version 36
//! stores authored taper values, and version 37 stores granted perks.

use std::collections::BTreeMap;

use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::{ActiveEffect, EffectId, World};
use crate::world::save::{
    read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader,
};

impl World {
    pub(crate) fn write_magic(&self, w: &mut Writer) {
        w.u64(self.next_effect);
        w.len(self.active_effects.len());
        for (id, effect) in &self.active_effects {
            w.u64(id.0);
            write_form_id(w, effect.spell);
            write_form_id(w, effect.effect);
            w.u32(effect.effect_index);
            write_form_id(w, effect.source);
            write_opt_form_id(w, effect.caster);
            write_form_id(w, effect.target);
            w.f32(effect.magnitude);
            w.f32(effect.taper_weight);
            w.f32(effect.taper_curve);
            w.f64(effect.taper_duration);
            w.bool(effect.duration.is_some());
            if let Some(duration) = effect.duration {
                w.f64(duration);
            }
            w.f64(effect.elapsed);
            write_opt_form_id(w, effect.summoned);
            write_opt_form_id(w, effect.granted_perk);
            w.bool(effect.recover);
            w.len(effect.changes.len());
            for (name, amount) in &effect.changes {
                w.str(name);
                w.f32(*amount);
            }
        }
    }
}

pub(in crate::world) fn read_magic(
    r: &mut FormReader<'_, '_>,
    has_summoned_reference: bool,
    has_taper_values: bool,
    has_granted_perk: bool,
) -> Result<(u64, BTreeMap<EffectId, ActiveEffect>), VmError> {
    let next = r.u64()?;
    let count = r.len()?;
    let mut effects = BTreeMap::new();
    for _ in 0..count {
        let id = EffectId(r.u64()?);
        let spell = read_form_id(r)?;
        let effect = read_form_id(r)?;
        let effect_index = r.u32()?;
        let source = read_form_id(r)?;
        let caster = read_opt_form_id(r)?;
        let target = read_form_id(r)?;
        let magnitude = r.f32()?;
        let (taper_weight, taper_curve, taper_duration) = if has_taper_values {
            (r.f32()?, r.f32()?, r.f64()?)
        } else {
            (1.0, 0.0, 0.0)
        };
        let duration = if r.bool()? { Some(r.f64()?) } else { None };
        let elapsed = r.f64()?;
        let summoned = if has_summoned_reference {
            read_opt_form_id(r)?
        } else {
            None
        };
        let granted_perk = if has_granted_perk {
            read_opt_form_id(r)?
        } else {
            None
        };
        let recover = r.bool()?;
        if id.0 == 0
            || id.0 >= next
            || !magnitude.is_finite()
            || magnitude < 0.0
            || !taper_weight.is_finite()
            || !taper_curve.is_finite()
            || !taper_duration.is_finite()
            || taper_duration < 0.0
            || !elapsed.is_finite()
            || elapsed < 0.0
            || duration.is_some_and(|d| !d.is_finite() || d + taper_duration < elapsed)
            || (taper_duration > 0.0 && taper_curve <= -1.0)
        {
            return Err(VmError::State("invalid active effect state".into()));
        }
        let count = r.len()?;
        if count > 2 {
            return Err(VmError::State("invalid active effect value count".into()));
        }
        let mut changes = Vec::new();
        for _ in 0..count {
            let name = r.str()?;
            let amount = r.f32()?;
            if !amount.is_finite() || crate::actor_values::actor_value_index(&name).is_none() {
                return Err(VmError::State("invalid active effect value".into()));
            }
            changes.push((name, amount));
        }
        let state = ActiveEffect {
            id,
            spell,
            effect,
            effect_index,
            source,
            caster,
            target,
            magnitude,
            duration,
            taper_weight,
            taper_curve,
            taper_duration,
            elapsed,
            summoned,
            granted_perk,
            recover,
            changes,
        };
        if effects.insert(id, state).is_some() {
            return Err(VmError::State("duplicate active effect identity".into()));
        }
    }
    if next == 0 {
        return Err(VmError::State("invalid next effect identity".into()));
    }
    Ok((next, effects))
}
