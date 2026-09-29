// SPDX-License-Identifier: GPL-3.0-or-later

//! Actors: the `NPC_` view with its template applied, the run-time state
//! built from it on first use, and actor values.

use std::collections::BTreeSet;

use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use soe_esm::{Error, GlobalFormId, Tag};

use super::{ActorState, World};
use crate::moddata::Distributed;
use crate::npc::Npc;
use soe_moddata::spid::RecordType;

/// One distributed entry onto a freshly built actor. Item counts land on the
/// reference's inventory rather than here, see `World::base_inventory`.
fn apply_distributed(a: &mut ActorState, d: Distributed) {
    match d.record_type {
        RecordType::Spell | RecordType::LevSpell => {
            a.spells.insert(d.what);
        }
        RecordType::Perk => {
            a.perks.insert(d.what);
        }
        RecordType::Outfit => a.outfit = Some(d.what),
        RecordType::SleepOutfit => a.sleep_outfit = Some(d.what),
        RecordType::Faction => {
            a.factions.entry(d.what).or_insert(0);
        }
        // Items reach the reference's inventory in `base_inventory`, not the
        // actor state. A package would need a third source in the driver's
        // stack and a save format change, and shouts, skins and bare forms
        // have no actor state at all yet. Each is held in `mod_data` and
        // counted, never silently dropped, and `docs/moddata.md` lists them.
        RecordType::Item
        | RecordType::Package
        | RecordType::Shout
        | RecordType::Skin
        | RecordType::Keyword
        | RecordType::Form => {}
    }
}

impl World {
    /// Current head-part and morph selections for the player.
    pub fn player_appearance(&mut self) -> Option<crate::appearance::PlayerAppearance> {
        let player = self.player();
        let actor = self.actor(player)?;
        Some(crate::appearance::PlayerAppearance {
            head_parts: actor.head_parts.clone(),
            morphs: actor.face_morphs.clone(),
        })
    }

    /// Set one playable HDPT of the requested part type for the player's sex.
    ///
    /// # Errors
    /// Rejects missing, deleted, non-playable, sex-incompatible or wrong-type HDPT records.
    pub fn set_player_head_part(
        &mut self,
        part_type: u32,
        part: GlobalFormId,
    ) -> Result<(), Error> {
        let player = self.player();
        let female = self
            .player_character_traits()
            .ok_or_else(|| Error::Invalid("player has no character traits".into()))?
            .1;
        self.validate_player_head_part(part_type, part, female)?;
        let actor = self
            .actor(player)
            .ok_or_else(|| Error::Invalid("player is not an actor".into()))?;
        actor.head_parts.insert(part_type, part);
        Ok(())
    }

    /// Validate a playable head part for a selected sex without changing the world.
    pub fn validate_player_head_part(
        &self,
        part_type: u32,
        part: GlobalFormId,
        female: bool,
    ) -> Result<(), Error> {
        let selected = self
            .record(part)
            .filter(|record| record.tag == Tag(*b"HDPT") && !record.is_deleted())
            .ok_or_else(|| Error::Invalid("missing player head part".into()))?;
        let data = selected
            .find(Tag(*b"DATA"))
            .filter(|data| data.len() == 1)
            .ok_or_else(|| Error::Invalid("invalid player head part flags".into()))?;
        let type_bytes = selected
            .find(Tag(*b"PNAM"))
            .filter(|bytes| bytes.len() == 4)
            .ok_or_else(|| Error::Invalid("invalid player head part type".into()))?;
        let flags = data[0];
        if flags & 1 == 0 {
            return Err(Error::Invalid("player head part is not playable".into()));
        }
        let sex_flag = if female { 4 } else { 2 };
        if flags & sex_flag == 0 {
            return Err(Error::Invalid("player head part does not match sex".into()));
        }
        if u32::from_le_bytes([type_bytes[0], type_bytes[1], type_bytes[2], type_bytes[3]])
            != part_type
        {
            return Err(Error::Invalid("player head part type mismatch".into()));
        }
        Ok(())
    }

    /// Replace a player morph value in the supported normalized range.
    ///
    /// # Errors
    /// Rejects empty names, non-finite values, values outside -1 to 1 and missing player actors.
    pub fn set_player_face_morph(&mut self, name: &str, value: f32) -> Result<(), Error> {
        if name.trim().is_empty() || !value.is_finite() || !(-1.0..=1.0).contains(&value) {
            return Err(Error::Invalid("invalid player face morph".into()));
        }
        let player = self.player();
        let actor = self
            .actor(player)
            .ok_or_else(|| Error::Invalid("player is not an actor".into()))?;
        actor.face_morphs.insert(name.to_owned(), value);
        Ok(())
    }

    /// Return the player's selected race and sex, initializing the base view from its record.
    pub fn player_character_traits(&mut self) -> Option<(GlobalFormId, bool)> {
        let player = self.player();
        let base = self.reference_ref(player)?.base?;
        let state = self.actor_base(base)?;
        Some((state.race?, state.female))
    }

    /// Apply a playable race and sex to the player without rebuilding initialized actor values.
    ///
    /// # Errors
    /// Rejects a missing player base or a race record that is not playable.
    pub fn set_player_character_traits(
        &mut self,
        race: GlobalFormId,
        female: bool,
    ) -> Result<(), Error> {
        let data = self
            .record(race)
            .filter(|record| record.tag == Tag(*b"RACE") && !record.is_deleted())
            .and_then(|record| record.find(Tag(*b"DATA")))
            .ok_or_else(|| Error::Invalid("missing player race data".into()))?;
        if data.len() < 36 || u32::from_le_bytes([data[32], data[33], data[34], data[35]]) & 1 == 0
        {
            return Err(Error::Invalid("player race is not playable".into()));
        }
        let player = self.player();
        let base = self
            .reference_ref(player)
            .and_then(|reference| reference.base)
            .ok_or_else(|| Error::Invalid("player has no NPC base".into()))?;
        let state = self
            .actor_base(base)
            .ok_or_else(|| Error::Invalid("player NPC base is invalid".into()))?;
        state.race = Some(race);
        state.female = female;
        if let Some(actor) = self.actors.get_mut(&player) {
            actor.race = Some(race);
        }
        Ok(())
    }

    /// The `NPC_` view of an actor base, with one level of `TPLT` applied.
    pub fn npc(&self, base: GlobalFormId) -> Option<Npc> {
        let mut npc = self.npc_record(base).ok()?;
        if npc.config.template_flags != 0 {
            if let Some(raw) = npc.template {
                let tid = self.resolve_from(base, raw)?;
                if let Ok(template) = self.npc_record(tid) {
                    npc.apply_template(&template);
                }
            }
        }
        Some(npc)
    }

    /// Run-time state of an actor, built from its `NPC_` base on first use.
    /// Without a base the three main values start at 100. Authored actors
    /// use the winning race and the NPC that supplies each template group.
    pub fn actor(&mut self, id: GlobalFormId) -> Option<&mut ActorState> {
        let base = match self.reference(id) {
            Some(r) if r.is_actor => r.base,
            _ => return None,
        };
        if self.actors.contains_key(&id) {
            let actor = self.actors.get(&id)?;
            if actor
                .spells
                .iter()
                .any(|spell| self.tag_of(*spell) == Some(super::T(b"LVSP")))
            {
                let roots = actor.spells.clone();
                let level = actor.level;
                let spells = self.expand_actor_spell_lists(&roots, level).ok()?;
                self.actors.get_mut(&id)?.spells = spells;
            }
        } else {
            let mut a = ActorState::default();
            if base.is_some() {
                use crate::npc::template;
                let (stats_source, stats) = self.actor_npc_part(id, template::STATS).ok()?;
                let (flags_source, flags) = self.actor_npc_part(id, template::BASE_DATA).ok()?;
                let (spells_source, spells) = self.actor_npc_part(id, template::SPELLS).ok()?;
                let (factions_source, factions) =
                    self.actor_npc_part(id, template::FACTIONS).ok()?;
                let (inventory_source, inventory) =
                    self.actor_npc_part(id, template::INVENTORY).ok()?;
                let traits_source = self.actor_component_source(id, template::TRAITS).ok()?;
                let (_, ai_source) = self.actor_npc_part(id, template::AI_DATA).ok()?;
                if let Some(data) = ai_source.ai_data {
                    for (name, value) in [
                        ("aggression", data.aggression),
                        ("confidence", data.confidence),
                        ("energy", data.energy),
                        ("morality", data.morality),
                        ("mood", data.mood),
                        ("assistance", data.assistance),
                    ] {
                        a.values.set_base(name, f32::from(value)).ok()?;
                    }
                }
                let race_override = base
                    .and_then(|base| self.actor_bases.get(&base))
                    .and_then(|state| state.race);
                for (name, value) in self
                    .actor_values_from_sources(stats_source, traits_source, race_override)
                    .ok()?
                {
                    a.values.set_base(&name, value).ok()?;
                }
                a.level = stats.level_for(self.player_level);
                a.essential = self
                    .actor_bases
                    .get(&flags_source)
                    .map_or_else(|| flags.is_essential(), |base| base.essential);
                a.protected = self
                    .actor_bases
                    .get(&flags_source)
                    .map_or_else(|| flags.is_protected(), |base| base.protected);
                a.ghost = flags.config.flags & crate::npc::flags::GHOST != 0;
                a.race = race_override.or(self.npc_race(traits_source).ok()?);
                a.perks = spells
                    .perks
                    .iter()
                    .filter_map(|raw| self.resolve_from(spells_source, *raw))
                    .collect();
                a.spells = spells
                    .spells
                    .iter()
                    .filter_map(|raw| self.resolve_from(spells_source, *raw))
                    .collect();
                let shouts: BTreeSet<_> = a
                    .spells
                    .iter()
                    .copied()
                    .filter(|id| self.tag_of(*id) == Some(super::T(b"SHOU")))
                    .collect();
                for shout in &shouts {
                    a.spells.remove(shout);
                }
                a.shouts = shouts;
                a.factions = factions
                    .factions
                    .iter()
                    .filter_map(|(raw, rank)| {
                        self.resolve_from(factions_source, *raw)
                            .map(|id| (id, i32::from(*rank)))
                    })
                    .collect();
                a.crime_faction = factions
                    .crime_faction
                    .and_then(|raw| self.resolve_from(factions_source, raw));
                a.outfit = inventory
                    .outfit
                    .and_then(|raw| self.resolve_from(inventory_source, raw));
                a.sleep_outfit = inventory
                    .sleep_outfit
                    .and_then(|raw| self.resolve_from(inventory_source, raw));
            } else {
                for name in ["health", "magicka", "stamina"] {
                    a.values.set_base(name, 100.0).ok()?;
                }
                a.level = 1;
            }
            a.equipped = self
                .refs
                .get(&id)
                .into_iter()
                .flat_map(|reference| {
                    reference.inventory.iter().filter_map(|(item, count)| {
                        (count > 0
                            && reference
                                .inventory
                                .stacks(*item)
                                .iter()
                                .any(|stack| stack.worn()))
                        .then_some(*item)
                    })
                })
                .collect();
            a.allow_pc_dialogue = true;
            // Spell Perk Item Distributor hands things to a base, and an
            // actor's state is built here on first use, so a reference that
            // loads long after the distribution still receives what its base
            // was given. See `crate::moddata`.
            if let Some(base) = base {
                for d in self.mod_data.for_base(base).to_vec() {
                    apply_distributed(&mut a, d);
                }
            }
            a.spells = self.expand_actor_spell_lists(&a.spells, a.level).ok()?;
            self.actors.insert(id, a);
        }
        self.actors.get_mut(&id)
    }

    /// The actor's state when it has been built, without building it.
    pub fn actor_ref(&self, id: GlobalFormId) -> Option<&ActorState> {
        self.actors.get(&id)
    }

    pub fn actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        self.actor(id)
            .map_or(0.0, |actor| actor.values.get(name).current())
    }

    pub fn base_actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        self.actor(id)
            .map_or(0.0, |actor| actor.values.get(name).base())
    }

    pub fn set_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        value: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(id, name, value, crate::actor_values::Values::set_base)
    }

    pub fn mod_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        delta: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(id, name, delta, crate::actor_values::Values::modify)
    }

    /// Current maximum, including permanent and temporary modifiers.
    pub fn actor_value_maximum(&mut self, id: GlobalFormId, name: &str) -> f32 {
        self.actor(id)
            .map_or(0.0, |actor| actor.values.get(name).maximum())
    }

    /// Current fraction of maximum, with the CK zero-maximum exception.
    pub fn actor_value_percentage(&mut self, id: GlobalFormId, name: &str) -> f32 {
        self.actor(id)
            .map_or(0.0, |actor| actor.values.percentage(name))
    }

    /// Force current value through its permanent modifier.
    ///
    /// # Errors
    /// Missing actors, non-finite input and overflow are rejected.
    pub fn force_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        value: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(id, name, value, crate::actor_values::Values::force)
    }

    /// Damage current value without reducing maximum.
    ///
    /// # Errors
    /// Missing actors, non-finite input and overflow are rejected.
    pub fn damage_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        value: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(id, name, value, crate::actor_values::Values::damage)
    }

    /// Restore existing damage, clamped to the current maximum.
    ///
    /// # Errors
    /// Missing actors and non-finite input are rejected.
    pub fn restore_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        value: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(id, name, value, crate::actor_values::Values::restore)
    }

    /// Apply an effect's temporary contribution to the shared value store.
    ///
    /// # Errors
    /// Missing actors, non-finite input and overflow are rejected.
    pub fn mod_temporary_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        value: f32,
    ) -> Result<(), crate::actor_values::Error> {
        self.change_actor_value(
            id,
            name,
            value,
            crate::actor_values::Values::modify_temporary,
        )
    }

    /// Clear health damage and death identity, retaining value modifiers.
    ///
    /// # Errors
    /// Returns an error when the form is not an actor.
    pub fn resurrect_actor(&mut self, id: GlobalFormId) -> Result<(), crate::actor_values::Error> {
        let actor = self
            .actor(id)
            .ok_or(crate::actor_values::Error::Actor(id))?;
        actor.values.restore("health", f32::MAX)?;
        actor.dead = false;
        actor.killer = None;
        self.pending.retain(
            |event| !matches!(event, super::Pending::DeferredDeath { actor, .. } if *actor == id),
        );
        Ok(())
    }

    fn change_actor_value(
        &mut self,
        id: GlobalFormId,
        name: &str,
        amount: f32,
        operation: fn(
            &mut crate::actor_values::Values,
            &str,
            f32,
        ) -> Result<(), crate::actor_values::Error>,
    ) -> Result<(), crate::actor_values::Error> {
        let actor = self
            .actor(id)
            .ok_or(crate::actor_values::Error::Actor(id))?;
        let is_health = name.eq_ignore_ascii_case("health");
        let (protected, essential) = (actor.protected, actor.essential);
        // Apply to a copy first, matching melee_hit's clone-then-clamp
        // pattern (combat.rs, CMB-13): the clamp below must see the health
        // change before it reaches the live actor, and every `Values`
        // setter already validates "without changing state" on its own, so
        // cloning costs nothing a direct call would not already pay.
        let mut values = actor.values.clone();
        let before = values.get("health").current();
        operation(&mut values, name, amount)?;
        let mut entering_bleedout = false;
        if is_health {
            let after = values.get("health").current();
            // A scripted DamageActorValue call carries no caster identity,
            // so unlike melee_hit's player_exception (combat.rs, CMB-34)
            // and apply_effect_values's caster check (magic.rs, CMB-35), a
            // merely protected actor is always clamped here rather than
            // left killable by an unidentified script.
            let bleedout_target = (protected || essential) && after < before;
            let (clamped, entering) = self.clamp_health_at_bleedout(id, bleedout_target, after);
            if entering {
                // `Values::restore` alone only raises the `damage` component,
                // so it silently no-ops for `SetActorValue`, `ModActorValue`,
                // `ForceActorValue` and `ModTemporaryActorValue`, which write
                // `base`, `permanent` or `temporary` instead. The commit
                // helper uses `damage` when the maximum allows it and falls
                // back to `permanent` only when the maximum itself fell below
                // the threshold (bleedout.rs, CMB-35 round 2).
                super::bleedout::commit_bleedout_health(&mut values, clamped)?;
                entering_bleedout = true;
            }
        }
        if let Some(actor) = self.actor(id) {
            actor.values = values;
            if entering_bleedout {
                actor.unconscious = true;
                actor.in_combat = false;
                actor.combat_target = None;
            }
        }
        if is_health {
            if !entering_bleedout {
                self.resolve_health_death(id);
            }
            self.resolve_bleedout_recovery(id);
        }
        Ok(())
    }
}

pub(crate) fn write_ui_14(actor: &super::ActorState, writer: &mut Writer) {
    writer.len(actor.head_parts.len());
    for (part_type, part) in &actor.head_parts {
        writer.u32(*part_type);
        super::save::write_form_id(writer, *part);
    }
    writer.len(actor.face_morphs.len());
    for (name, value) in &actor.face_morphs {
        writer.str(name);
        writer.f32(*value);
    }
}

pub(crate) fn read_ui_14(
    actor: &mut super::ActorState,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), VmError> {
    let part_count = reader.len()?;
    for _ in 0..part_count {
        let part_type = reader.u32()?;
        let part = super::save::read_form_id(reader)?;
        if actor.head_parts.insert(part_type, part).is_some() {
            return Err(VmError::State("duplicate UI-14 head part type".into()));
        }
    }
    let morph_count = reader.len()?;
    for _ in 0..morph_count {
        let name = reader.str()?;
        let value = reader.f32()?;
        if name.trim().is_empty() || !value.is_finite() || !(-1.0..=1.0).contains(&value) {
            return Err(VmError::State("invalid UI-14 morph value".into()));
        }
        if actor.face_morphs.insert(name, value).is_some() {
            return Err(VmError::State("duplicate UI-14 morph name".into()));
        }
    }
    Ok(())
}
