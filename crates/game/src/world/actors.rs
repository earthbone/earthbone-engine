// SPDX-License-Identifier: GPL-3.0-or-later

//! Actors: the `NPC_` view with its template applied, the run-time state
//! built from it on first use, and actor values.

use std::collections::BTreeSet;

use soe_esm::GlobalFormId;

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
                for (name, value) in self
                    .actor_values_from_sources(stats_source, traits_source)
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
                a.race = self.npc_race(traits_source).ok()?;
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

    /// Whether an instantiated actor is currently marked alarmed.
    ///
    /// # Errors
    /// This query does not fail.
    ///
    /// # Panics
    /// This query does not panic.
    #[must_use]
    pub fn actor_is_alarmed(&self, id: GlobalFormId) -> bool {
        self.actor_ref(id).is_some_and(|actor| actor.alerted)
    }

    /// Whether an instantiated actor is unconscious at or below its bleedout threshold.
    /// Missing or invalid bleedout threshold data returns `false`.
    ///
    /// # Panics
    /// This query does not panic.
    #[must_use]
    pub fn actor_is_bleeding_out(&mut self, id: GlobalFormId) -> bool {
        let Some(actor) = self.actor_ref(id) else {
            return false;
        };
        if actor.dead || !actor.unconscious {
            return false;
        }
        let health = actor.values.get("health").current();
        self.actor_bleedout_threshold(id)
            .is_ok_and(|threshold| health <= threshold)
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
