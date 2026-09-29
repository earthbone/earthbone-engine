// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW version 5 stores active applications after the version 4 world body.
//! Loaded actor values already contain recoverable modifiers.

use std::collections::BTreeMap;

use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::{ActiveEffect, EffectId, World};
use crate::world::save::{
    read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader,
};
use crate::world::{ActorState, WerewolfTransformState};

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
            w.bool(effect.duration.is_some());
            if let Some(duration) = effect.duration {
                w.f64(duration);
            }
            w.f64(effect.elapsed);
            write_opt_form_id(w, effect.summoned);
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
        let duration = if r.bool()? { Some(r.f64()?) } else { None };
        let elapsed = r.f64()?;
        let summoned = if has_summoned_reference {
            read_opt_form_id(r)?
        } else {
            None
        };
        let recover = r.bool()?;
        if id.0 == 0
            || id.0 >= next
            || !magnitude.is_finite()
            || magnitude < 0.0
            || !elapsed.is_finite()
            || elapsed < 0.0
            || duration.is_some_and(|d| !d.is_finite() || d < elapsed)
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
            elapsed,
            summoned,
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

pub(in crate::world) fn write_werewolf_transform(actor: &ActorState, w: &mut Writer) {
    let Some(transform) = &actor.werewolf_transform else {
        w.bool(false);
        return;
    };
    w.bool(true);
    w.u64(transform.effect.0);
    write_opt_form_id(w, transform.prior_race);
    write_form_id(w, transform.transformed_race);
    w.len(transform.added_spells.len());
    for spell in &transform.added_spells {
        write_form_id(w, *spell);
    }
    w.len(transform.added_shouts.len());
    for shout in &transform.added_shouts {
        write_form_id(w, *shout);
    }
    w.len(transform.added_effects.len());
    for effect in &transform.added_effects {
        w.u64(effect.0);
    }
}

pub(in crate::world) fn read_werewolf_transform(
    actor: &mut ActorState,
    r: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    if !r.bool()? {
        return Ok(());
    }
    let effect = EffectId(r.u64()?);
    let prior_race = read_opt_form_id(r)?;
    let transformed_race = read_form_id(r)?;
    let count = r.len()?;
    let mut added_spells = std::collections::BTreeSet::new();
    for _ in 0..count {
        if !added_spells.insert(read_form_id(r)?) {
            return Err(VmError::State("duplicate werewolf race effect".into()));
        }
    }
    let count = r.len()?;
    let mut added_shouts = std::collections::BTreeSet::new();
    for _ in 0..count {
        if !added_shouts.insert(read_form_id(r)?) {
            return Err(VmError::State("duplicate werewolf race shout".into()));
        }
    }
    let count = r.len()?;
    let mut added_effects = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = EffectId(r.u64()?);
        if id.0 == 0 || !added_effects.insert(id) {
            return Err(VmError::State("invalid owned werewolf effect".into()));
        }
    }
    if effect.0 == 0 || actor.race != Some(transformed_race) || transformed_race.object == 0 {
        return Err(VmError::State("invalid werewolf transform state".into()));
    }
    actor.werewolf_transform = Some(WerewolfTransformState {
        effect,
        prior_race,
        transformed_race,
        added_spells,
        added_shouts,
        added_effects,
    });
    Ok(())
}
