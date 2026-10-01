// SPDX-License-Identifier: GPL-3.0-or-later

//! Death requests follow CK Start/EndDeferredKill and KillSilent contracts.
//! Actor.psc declares the OnDying then OnDeath lifecycle. Rendering remains separate.

use std::collections::{HashMap, HashSet};

use soe_esm::GlobalFormId;
use soe_papyrus::VmError;

use super::{ActorState, Handle, Pending, World};

impl World {
    /// Update the source base and every initialized actor inheriting its essential flag.
    pub fn set_actor_base_essential(&mut self, base: GlobalFormId, essential: bool) {
        let Some(state) = self.actor_base(base) else {
            return;
        };
        state.essential = essential;
        let mut actors: Vec<_> = self.actors.keys().copied().collect();
        actors.sort_unstable();
        for actor in actors {
            if self.actor_component_source(actor, crate::npc::template::BASE_DATA) == Ok(base) {
                if let Some(state) = self.actors.get_mut(&actor) {
                    state.essential = essential;
                }
            }
        }
    }

    /// Update the source base and every initialized actor inheriting its protected flag.
    /// Mirrors `set_actor_base_essential` (CMB-34): `World::melee_hit` also
    /// reads the base fresh on every hit, so this propagation is a
    /// consistency fix for `Actor.IsProtected` and saved state, not the
    /// path combat correctness depends on.
    pub fn set_actor_base_protected(&mut self, base: GlobalFormId, protected: bool) {
        let Some(state) = self.actor_base(base) else {
            return;
        };
        state.protected = protected;
        let mut actors: Vec<_> = self.actors.keys().copied().collect();
        actors.sort_unstable();
        for actor in actors {
            if self.actor_component_source(actor, crate::npc::template::BASE_DATA) == Ok(base) {
                if let Some(state) = self.actors.get_mut(&actor) {
                    state.protected = protected;
                }
            }
        }
    }

    /// Kill an actor, or retain the request while death is deferred.
    /// Essential and already-dead actors do not begin another death transition.
    /// Returns true only when death occurs during this call.
    pub fn kill_actor(&mut self, actor: GlobalFormId, killer: Option<GlobalFormId>) -> bool {
        self.request_actor_death(actor, killer, false)
    }

    /// Kill without queuing death notifications, retaining that choice when deferred.
    pub fn kill_actor_silent(&mut self, actor: GlobalFormId, killer: Option<GlobalFormId>) -> bool {
        self.request_actor_death(actor, killer, true)
    }

    fn request_actor_death(
        &mut self,
        id: GlobalFormId,
        killer: Option<GlobalFormId>,
        silent: bool,
    ) -> bool {
        let Some(actor) = self.actor(id) else {
            return false;
        };
        if actor.dead || actor.essential {
            return false;
        }
        let health = actor.values.get("health").current().max(0.0);
        if actor.values.damage("health", health).is_err() {
            return false;
        }
        if actor.deferred_kill {
            if !self
                .pending
                .iter()
                .any(|event| matches!(event, Pending::DeferredDeath { actor, .. } if *actor == id))
            {
                self.pending.push(Pending::DeferredDeath {
                    actor: id,
                    killer,
                    silent,
                });
            }
            return false;
        }
        self.finish_actor_death(id, killer, silent)
    }

    fn finish_actor_death(
        &mut self,
        id: GlobalFormId,
        killer: Option<GlobalFormId>,
        silent: bool,
    ) -> bool {
        let Some(actor) = self.actor(id) else {
            return false;
        };
        if actor.dead || actor.essential {
            return false;
        }
        let health = actor.values.get("health").current().max(0.0);
        if actor.values.damage("health", health).is_err() {
            return false;
        }
        actor.dead = true;
        actor.killer = killer;
        actor.in_combat = false;
        actor.combat_target = None;
        if !silent {
            if let Some(killer) = killer.filter(|killer| self.actor(*killer).is_some()) {
                let event = soe_commonlib_shim::events::ActorKill {
                    killer: World::event_form_id(killer),
                    victim: World::event_form_id(id),
                };
                self.actor_kill_events.send(&event);
            }
            let recipients = self.actor_event_recipients(id);
            self.pending.push(Pending::ActorDeath {
                actor: id,
                killer,
                recipients,
            });
        }
        true
    }

    /// Hold future lethal requests until EndDeferredKill, without reviving a corpse.
    pub fn start_deferred_kill(&mut self, id: GlobalFormId) {
        if let Some(actor) = self.actor(id).filter(|actor| !actor.dead) {
            actor.deferred_kill = true;
        }
    }

    /// Resolve the first held lethal request, including after healing or save/load.
    /// Old engine saves without a held request still resolve nonpositive health.
    pub fn end_deferred_kill(&mut self, id: GlobalFormId) -> bool {
        let Some(actor) = self.actor(id) else {
            return false;
        };
        if !actor.deferred_kill {
            return false;
        }
        actor.deferred_kill = false;
        let lethal = actor.values.get("health").current() <= 0.0;
        let request = self.pending.iter().position(
            |event| matches!(event, Pending::DeferredDeath { actor, .. } if *actor == id),
        );
        if let Some(index) = request {
            if let Pending::DeferredDeath { killer, silent, .. } = self.pending.remove(index) {
                return self.finish_actor_death(id, killer, silent);
            }
        }
        lethal && self.finish_actor_death(id, None, false)
    }

    pub(super) fn resolve_health_death(&mut self, id: GlobalFormId) {
        if self
            .actor_ref(id)
            .is_some_and(|actor| actor.values.get("health").current() <= 0.0)
        {
            self.request_actor_death(id, None, false);
        }
    }

    pub(super) fn actor_event_recipients(&self, actor: GlobalFormId) -> Vec<Handle> {
        let player_alias = (self.player == Some(actor)).then_some(GlobalFormId {
            file: 0,
            object: super::PLAYER_OBJECT,
        });
        let mut aliases: Vec<_> = self
            .quests
            .iter()
            .filter(|(_, state)| state.running)
            .flat_map(|(&quest, state)| {
                state.aliases.iter().filter_map(move |(&id, &reference)| {
                    reference
                        .is_some_and(|reference| {
                            reference == actor || Some(reference) == player_alias
                        })
                        .then_some((quest, id))
                })
            })
            .collect();
        aliases.sort_unstable();
        let mut recipients = vec![Handle::Form(actor)];
        recipients.extend(
            aliases
                .into_iter()
                .map(|(quest, id)| Handle::Alias { quest, id }),
        );
        recipients.extend(
            self.active_effects
                .iter()
                .filter(|(_, effect)| effect.target == actor)
                .map(|(&id, _)| Handle::ActiveEffect(id)),
        );
        recipients
    }

    /// Held death requests are persistent state, not runnable VM work.
    pub fn has_pending_calls(&self) -> bool {
        self.pending
            .iter()
            .any(|event| !matches!(event, Pending::DeferredDeath { .. }))
    }
}

pub(super) fn validate_pending(
    actors: &HashMap<GlobalFormId, ActorState>,
    pending: &[Pending],
) -> Result<(), VmError> {
    let bad = || VmError::State("inconsistent pending actor death".into());
    let mut held = HashSet::new();
    for event in pending {
        match event {
            Pending::DeferredDeath { actor, .. }
                if actors
                    .get(actor)
                    .is_none_or(|state| state.dead || !state.deferred_kill)
                    || !held.insert(*actor) =>
            {
                return Err(bad())
            }
            Pending::ActorDeath {
                actor, recipients, ..
            } => {
                let unique: HashSet<_> = recipients.iter().collect();
                if !actors.contains_key(actor)
                    || recipients.first() != Some(&Handle::Form(*actor))
                    || unique.len() != recipients.len()
                {
                    return Err(bad());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
