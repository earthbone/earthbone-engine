// SPDX-License-Identifier: GPL-3.0-or-later

//! Actors: the `NPC_` view with its template applied, the run-time state
//! built from it on first use, and actor values.

use soe_esm::GlobalFormId;

use super::{ActorState, World};
use crate::npc::Npc;

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
    /// Without a base (the fallback player) the three main values start at
    /// 100. Race starting values are not read yet.
    pub fn actor(&mut self, id: GlobalFormId) -> Option<&mut ActorState> {
        let base = match self.reference(id) {
            Some(r) if r.is_actor => r.base,
            _ => return None,
        };
        if !self.actors.contains_key(&id) {
            let mut a = ActorState::default();
            match base.and_then(|b| self.npc(b)) {
                Some(npc) => {
                    let base_file = base.and_then(|b| self.order.winner_ref(b)).map(|r| r.file);
                    for (name, v) in npc.base_values() {
                        a.values.insert(name.clone(), v);
                        a.base_values.insert(name, v);
                    }
                    a.level = npc.level_for(self.player_level);
                    a.essential = npc.is_essential();
                    a.protected = npc.is_protected();
                    a.ghost = npc.config.flags & crate::npc::flags::GHOST != 0;
                    a.allow_pc_dialogue = true;
                    if let Some(file) = base_file {
                        a.race = npc.race.map(|r| self.order.resolve(file, r));
                        a.perks = npc
                            .perks
                            .iter()
                            .map(|p| self.order.resolve(file, *p))
                            .collect();
                        a.crime_faction = npc.crime_faction.map(|f| self.order.resolve(file, f));
                        a.outfit = npc.outfit.map(|o| self.order.resolve(file, o));
                        a.sleep_outfit = npc.sleep_outfit.map(|o| self.order.resolve(file, o));
                        a.spells = npc
                            .spells
                            .iter()
                            .map(|s| self.order.resolve(file, *s))
                            .collect();
                        a.factions = npc
                            .factions
                            .iter()
                            .map(|(f, rank)| (self.order.resolve(file, *f), i32::from(*rank)))
                            .collect();
                    }
                }
                None => {
                    for (name, v) in [("health", 100.0), ("magicka", 100.0), ("stamina", 100.0)] {
                        a.values.insert(name.into(), v);
                        a.base_values.insert(name.into(), v);
                    }
                    a.level = 1;
                    a.allow_pc_dialogue = true;
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

    pub fn actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        let key = name.to_ascii_lowercase();
        self.actor(id)
            .and_then(|a| a.values.get(&key).copied())
            .unwrap_or(0.0)
    }

    pub fn base_actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        let key = name.to_ascii_lowercase();
        self.actor(id)
            .and_then(|a| a.base_values.get(&key).copied())
            .unwrap_or(0.0)
    }

    pub fn set_actor_value(&mut self, id: GlobalFormId, name: &str, value: f32) {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.actor(id) {
            a.values.insert(key.clone(), value);
            a.base_values.insert(key, value);
        }
    }

    pub fn mod_actor_value(&mut self, id: GlobalFormId, name: &str, delta: f32) {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.actor(id) {
            *a.values.entry(key).or_insert(0.0) += delta;
            if key_is_health(name) && a.values["health"] <= 0.0 && !a.essential {
                a.dead = true;
            }
        }
    }
}

fn key_is_health(name: &str) -> bool {
    name.eq_ignore_ascii_case("health")
}
