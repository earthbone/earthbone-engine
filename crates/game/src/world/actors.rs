// SPDX-License-Identifier: GPL-3.0-or-later

//! Actors: the `NPC_` view with its template applied, the run-time state
//! built from it on first use, and actor values.

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
        let rref = self.order.winner_ref(base)?;
        let rec = self.order.record(rref);
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        let mut npc = Npc::from_record(rec, localized).ok()?;
        if npc.config.template_flags != 0 {
            if let Some(raw) = npc.template {
                let tid = self.order.resolve(rref.file, raw);
                if let Some(tref) = self.order.winner_ref(tid) {
                    let trec = self.order.record(tref);
                    let tloc = self.order.file(tref.file).plugin.header.is_localized();
                    if let Ok(t) = Npc::from_record(trec, tloc) {
                        npc.apply_template(&t);
                    }
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
        if !self.actors.contains_key(&id) {
            let mut a = ActorState::default();
            if base.is_some() {
                use crate::npc::template;
                let (stats_source, stats) = self.actor_npc_part(id, template::STATS).ok()?;
                let (_, flags) = self.actor_npc_part(id, template::BASE_DATA).ok()?;
                let (spells_source, spells) = self.actor_npc_part(id, template::SPELLS).ok()?;
                let (factions_source, factions) =
                    self.actor_npc_part(id, template::FACTIONS).ok()?;
                let (inventory_source, inventory) =
                    self.actor_npc_part(id, template::INVENTORY).ok()?;
                let traits_source = self.actor_component_source(id, template::TRAITS).ok()?;
                for (name, value) in self
                    .actor_values_from_sources(stats_source, traits_source)
                    .ok()?
                {
                    a.values.set_base(&name, value).ok()?;
                }
                a.level = stats.level_for(self.player_level);
                a.essential = flags.is_essential();
                a.protected = flags.is_protected();
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
            self.actors.insert(id, a);
        }
        self.actors.get_mut(&id)
    }

    /// The actor's state when it has been built, without building it.
    pub fn actor_ref(&self, id: GlobalFormId) -> Option<&ActorState> {
        self.actors.get(&id)
    }

    /// `Actor.Kill` and `Actor.KillSilent`: an essential actor never dies,
    /// and one inside a deferred kill holds its death until
    /// `EndDeferredKill`. True when the actor died here.
    pub fn kill_actor(&mut self, id: GlobalFormId, killer: Option<GlobalFormId>) -> bool {
        let Some(a) = self.actor(id) else {
            return false;
        };
        if a.essential || a.deferred_kill {
            return false;
        }
        let current = a.values.get("health").current().max(0.0);
        if a.values.damage("health", current).is_err() {
            return false;
        }
        a.dead = true;
        a.killer = killer;
        true
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
        operation(&mut actor.values, name, amount)?;
        if name.eq_ignore_ascii_case("health")
            && actor.values.get(name).current() <= 0.0
            && !actor.essential
            && !actor.deferred_kill
        {
            actor.dead = true;
        }
        Ok(())
    }
}
