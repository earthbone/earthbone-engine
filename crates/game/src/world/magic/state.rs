// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW version 5 stores active applications after the version 4 world body.
//! Loaded actor values already contain recoverable modifiers.

use std::collections::BTreeMap;

use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::{ActiveEffect, EffectId, PriorOwner, World};
use crate::world::save::{
    read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader,
};

pub(crate) const SECTION: crate::world::save_sections::Section<World> =
    crate::world::save_sections::Section {
        name: "MAG-33",
        write: write_command_owners,
        read: read_command_owners,
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

fn write_command_owners(world: &World, w: &mut Writer) {
    let commanded: Vec<_> = world
        .active_effects
        .values()
        .filter_map(|effect| effect.restore_owner.map(|owner| (effect.id, owner)))
        .collect();
    w.len(commanded.len());
    for (id, owner) in commanded {
        w.u64(id.0);
        match owner {
            PriorOwner::Unowned => w.bool(false),
            PriorOwner::Owned(form) => {
                w.bool(true);
                write_form_id(w, form);
            }
        }
    }
}

fn read_command_owners(world: &mut World, r: &mut FormReader<'_, '_>) -> Result<(), VmError> {
    let count = r.len()?;
    if count > r.remaining() / 9 {
        return Err(VmError::State(
            "MAG-33 command count exceeds remaining bytes".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = EffectId(r.u64()?);
        if !seen.insert(id) {
            return Err(VmError::State("MAG-33 duplicate effect identity".into()));
        }
        let owner = if r.bool()? {
            PriorOwner::Owned(read_form_id(r)?)
        } else {
            PriorOwner::Unowned
        };
        let effect = world
            .active_effects
            .get_mut(&id)
            .ok_or_else(|| VmError::State("MAG-33 missing active effect".into()))?;
        effect.restore_owner = Some(owner);
    }
    Ok(())
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
            restore_owner: None,
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
