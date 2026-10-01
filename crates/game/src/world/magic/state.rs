// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW version 5 stores active applications after the version 4 world body.
//! Loaded actor values already contain recoverable modifiers.

use std::collections::BTreeMap;

use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use super::{ActiveEffect, CloakState, EffectId, PriorOwner, World};
use crate::world::save::{
    read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader,
};
use crate::world::{ActorState, WerewolfTransformState};

pub(crate) const SECTION: crate::world::save_sections::Section<World> =
    crate::world::save_sections::Section {
        name: "MAG-33",
        write: write_command_owners,
        read: read_command_owners,
    };

pub(crate) const REANIMATE_SECTION: crate::world::save_sections::Section<World> =
    crate::world::save_sections::Section {
        name: "MAG-43",
        write: write_reanimate_owners,
        read: read_reanimate_owners,
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

    pub(crate) fn write_cloaks(&self, w: &mut Writer) {
        w.len(self.cloak_effects.len());
        for (id, state) in &self.cloak_effects {
            w.u64(id.0);
            w.f64(state.elapsed);
            w.len(state.owned.len());
            for owned in &state.owned {
                w.u64(owned.0);
            }
        }
    }

    pub(crate) fn read_cloaks(&mut self, r: &mut FormReader<'_, '_>) -> Result<(), VmError> {
        let count = r.len()?;
        let mut cloaks = BTreeMap::new();
        for _ in 0..count {
            let id = EffectId(r.u64()?);
            let elapsed = r.f64()?;
            let owned_count = r.len()?;
            let mut owned = Vec::with_capacity(owned_count);
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..owned_count {
                let owned_id = EffectId(r.u64()?);
                if owned_id.0 == 0 || !seen.insert(owned_id) {
                    return Err(VmError::State("invalid cloak owned effect identity".into()));
                }
                owned.push(owned_id);
            }
            if !elapsed.is_finite()
                || !(0.0..1.0).contains(&elapsed)
                || id.0 == 0
                || cloaks.insert(id, CloakState { elapsed, owned }).is_some()
            {
                return Err(VmError::State("invalid cloak effect state".into()));
            }
        }
        self.cloak_effects = cloaks;
        Ok(())
    }

    pub(crate) fn validate_cloaks(&self) -> Result<(), VmError> {
        for (id, state) in &self.cloak_effects {
            let parent = self
                .active_effects
                .get(id)
                .ok_or_else(|| VmError::State("cloak state names a missing effect".into()))?;
            if !self
                .magic_effect(parent.effect)
                .is_ok_and(|base| base.data.archetype == 35)
                || state
                    .owned
                    .iter()
                    .any(|owned| !self.active_effects.contains_key(owned))
            {
                return Err(VmError::State("invalid cloak effect ownership".into()));
            }
        }
        Ok(())
    }
}

fn write_reanimate_owners(world: &World, w: &mut Writer, _pool: &mut ArrayPool) {
    let owners: Vec<_> = world
        .active_effects
        .values()
        .filter_map(|effect| {
            effect
                .reanimated_previous_owner
                .map(|owner| (effect.id, owner))
        })
        .collect();
    w.len(owners.len());
    for (id, owner) in owners {
        w.u64(id.0);
        write_opt_form_id(w, owner);
    }
}

fn read_reanimate_owners(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = r.len()?;
    if count > r.remaining() / 12 {
        return Err(VmError::State(
            "MAG-43 owner count exceeds remaining bytes".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = EffectId(r.u64()?);
        let owner = read_opt_form_id(r)?;
        if !seen.insert(id) {
            return Err(VmError::State("MAG-43 duplicate effect identity".into()));
        }
        let effect = world
            .active_effects
            .get_mut(&id)
            .ok_or_else(|| VmError::State("MAG-43 missing active effect".into()))?;
        effect.reanimated_previous_owner = Some(owner);
    }
    Ok(())
}

fn write_command_owners(world: &World, w: &mut Writer, _pool: &mut ArrayPool) {
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

fn read_command_owners(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
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
    has_reanimated_owner: bool,
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
        let reanimated_previous_owner = if has_reanimated_owner && r.bool()? {
            Some(read_opt_form_id(r)?)
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
            reanimated_previous_owner,
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
