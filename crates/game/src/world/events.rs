// SPDX-License-Identifier: GPL-3.0-or-later

//! Events natives raise for registered listeners: tracked stats, sleep,
//! animation events, line of sight, scenes and sound instances. Each one
//! queues into `pending` for the VM to deliver. SCEN action fields follow
//! xEdit `wbRecord(SCEN)` and the UESP SCEN layout. The local CK scene index is
//! in `Papyrus_Introduction.wikitext`.

use soe_esm::records::gameplay::{Scene, SceneActionData};
use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::{vm::LatentId, VmError};
use soe_papyrus::{ObjectId, Value};

use super::{Alloc, Handle, Pending, World};

pub(super) fn write_animation_waits(world: &World, writer: &mut Writer) {
    world.write_animation_waits(writer);
}

pub(super) fn read_animation_waits(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), VmError> {
    world.read_animation_waits(reader)
}

impl World {
    pub(super) fn write_animation_waits(&self, writer: &mut Writer) {
        writer.len(self.animation_waiters.len());
        for waiter in &self.animation_waiters {
            super::save::write_form_id(writer, waiter.source);
            writer.str(&waiter.event);
            writer.u64(waiter.latent.0);
        }
        writer.len(self.completed_animation_waits.len());
        for (latent, result) in &self.completed_animation_waits {
            writer.u64(latent.0);
            writer.bool(*result);
        }
    }

    pub(super) fn read_animation_waits(
        &mut self,
        reader: &mut super::save::FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        let count = reader.len()?;
        self.animation_waiters.clear();
        for _ in 0..count {
            self.animation_waiters.push(super::AnimationWaiter {
                source: super::save::read_form_id(reader)?,
                event: reader.str()?,
                latent: LatentId(reader.u64()?),
            });
        }
        let count = reader.len()?;
        self.completed_animation_waits.clear();
        for _ in 0..count {
            self.completed_animation_waits
                .push((LatentId(reader.u64()?), reader.bool()?));
        }
        Ok(())
    }
}

/// A transient request for the host to display one EffectShader on a reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectShaderRequest {
    pub shader: GlobalFormId,
    pub target: GlobalFormId,
    pub duration: f32,
    pub playing: bool,
}

impl World {
    /// The active scene whose quest alias currently names `reference`.
    pub fn current_scene(&mut self, reference: GlobalFormId) -> Option<GlobalFormId> {
        let scenes: Vec<_> = self.scenes.iter().copied().collect();
        for scene_id in scenes {
            let Some(record) = self.record(scene_id) else {
                continue;
            };
            let Ok(scene) = Scene::from_record(record) else {
                continue;
            };
            let Some(quest) = scene.quest.and_then(|raw| self.resolve_from(scene_id, raw)) else {
                continue;
            };
            if scene.actors.iter().any(|actor| {
                actor.alias_id >= 0
                    && self.alias_ref(quest, actor.alias_id as u32) == Some(reference)
            }) {
                return Some(scene_id);
            }
        }
        None
    }

    /// Queue a shader start or stop for the renderer. Repeated starts replace
    /// the lifetime for the same shader and target when the host consumes it.
    pub fn effect_shader(
        &mut self,
        shader: GlobalFormId,
        target: GlobalFormId,
        duration: f32,
        playing: bool,
    ) {
        self.effect_shader_requests.push(EffectShaderRequest {
            shader,
            target,
            duration,
            playing,
        });
    }

    /// Drain visual requests after the host has reconciled them.
    pub fn take_effect_shader_requests(&mut self) -> Vec<EffectShaderRequest> {
        std::mem::take(&mut self.effect_shader_requests)
    }

    /// Take queued Story Manager inputs for the scheduler.
    pub fn take_story_manager_events(&mut self) -> Vec<crate::quest::state::StoryManagerEvent> {
        std::mem::take(&mut self.story_manager_events)
    }

    /// An event to the scripts on `form`, the way `OnActivate` reaches a
    /// reference: `Actor.SendVampirismStateChanged` and its siblings use it.
    pub fn form_event(
        &mut self,
        form: GlobalFormId,
        name: &str,
        args: Vec<Value>,
        alloc: Alloc<'_>,
    ) {
        let object = self.object_of(Handle::Form(form), alloc);
        self.pending.push(Pending::Event {
            object,
            name: name.into(),
            args,
        });
    }

    /// `Game.IncrementStat`: the new value, and `OnTrackedStatsEvent` to
    /// every registered listener.
    pub fn increment_stat(&mut self, name: &str, amount: i32) -> i32 {
        let v = self.stats.entry(name.to_string()).or_insert(0);
        *v += amount;
        let value = *v;
        for object in self.stats_listeners.clone() {
            self.pending.push(Pending::Event {
                object,
                name: "OnTrackedStatsEvent".into(),
                args: vec![Value::string(name), Value::Int(value)],
            });
        }
        value
    }

    /// The player lies down: `OnSleepStart(start, end)` in game days to
    /// every sleep listener.
    pub fn sleep_start(&mut self, start: f32, end: f32) {
        self.sleep_event("OnSleepStart", &[Value::Float(start), Value::Float(end)]);
    }

    /// The player wakes: `OnSleepStop(interrupted)` to every sleep listener.
    pub fn sleep_stop(&mut self, interrupted: bool) {
        self.sleep_event("OnSleepStop", &[Value::Bool(interrupted)]);
    }

    fn sleep_event(&mut self, name: &str, args: &[Value]) {
        for object in self.sleep_listeners.clone() {
            self.pending.push(Pending::Event {
                object,
                name: name.into(),
                args: args.to_vec(),
            });
        }
    }

    /// An animation event fired on `source`: recorded, and delivered as
    /// `OnAnimationEvent(source, name)` to whoever registered for it.
    pub fn animation_event(&mut self, source: GlobalFormId, name: &str, alloc: Alloc<'_>) {
        self.animation_events.push((source, name.to_string()));
        self.finish_animation_waits(source, Some(name));
        let listeners: Vec<ObjectId> = self
            .animation_listeners
            .iter()
            .filter(|l| l.sender == source && l.event.eq_ignore_ascii_case(name))
            .map(|l| l.listener)
            .collect();
        if listeners.is_empty() {
            return;
        }
        let src = self.form_value(Some(source), alloc);
        for object in listeners {
            self.pending.push(Pending::Event {
                object,
                name: "OnAnimationEvent".into(),
                args: vec![src.clone(), Value::string(name)],
            });
        }
    }

    /// Register one latent wait. A reference that is already disabled
    /// completes immediately with false.
    pub fn wait_for_animation_event(
        &mut self,
        source: GlobalFormId,
        name: &str,
        latent: soe_papyrus::vm::LatentId,
    ) {
        if !self
            .reference(source)
            .is_some_and(|reference| reference.enabled)
        {
            self.completed_animation_waits.push((latent, false));
            return;
        }
        self.animation_waiters.push(super::AnimationWaiter {
            source,
            event: name.to_string(),
            latent,
        });
    }

    /// The graph for this reference stopped producing events.
    pub fn animation_graph_unloaded(&mut self, source: GlobalFormId) {
        self.finish_animation_waits(source, None);
    }

    /// Resolve waits when a reference becomes disabled.
    pub fn set_reference_enabled(&mut self, source: GlobalFormId, enabled: bool) {
        if let Some(reference) = self.reference(source) {
            reference.enabled = enabled;
        }
        if !enabled {
            self.finish_animation_waits(source, None);
        }
    }

    fn finish_animation_waits(&mut self, source: GlobalFormId, event: Option<&str>) {
        let mut remaining = Vec::with_capacity(self.animation_waiters.len());
        for waiter in std::mem::take(&mut self.animation_waiters) {
            if waiter.source != source {
                remaining.push(waiter);
            } else if let Some(name) = event {
                if waiter.event.eq_ignore_ascii_case(name) {
                    self.completed_animation_waits.push((waiter.latent, true));
                } else {
                    remaining.push(waiter);
                }
            } else {
                self.completed_animation_waits.push((waiter.latent, false));
            }
        }
        self.animation_waiters = remaining;
    }

    /// A viewer gains or loses sight of a target: `OnGainLOS` or
    /// `OnLostLOS` to the matching registrations, single ones consumed.
    pub fn line_of_sight(
        &mut self,
        viewer: GlobalFormId,
        target: GlobalFormId,
        gained: bool,
        alloc: Alloc<'_>,
    ) {
        let hits: Vec<ObjectId> = self
            .los_listeners
            .iter()
            .filter(|l| l.viewer == viewer && l.target == target && (!l.single || l.gain == gained))
            .map(|l| l.listener)
            .collect();
        self.los_listeners.retain(|l| {
            !(l.viewer == viewer && l.target == target && l.single && l.gain == gained)
        });
        if hits.is_empty() {
            return;
        }
        let v = self.form_value(Some(viewer), alloc);
        let t = self.form_value(Some(target), alloc);
        let name = if gained { "OnGainLOS" } else { "OnLostLOS" };
        for object in hits {
            self.pending.push(Pending::Event {
                object,
                name: name.into(),
                args: vec![v.clone(), t.clone()],
            });
        }
    }

    /// `Scene.Start`: playing, and `OnBegin` to the scene's scripts.
    pub fn start_scene(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        let Some(record) = self.record(id) else {
            return false;
        };
        let Ok(scene) = Scene::from_record(record) else {
            return false;
        };
        if scene.phases.is_empty()
            || scene
                .actions
                .iter()
                .any(|action| match (action.start_phase, action.end_phase) {
                    (Some(start), Some(end)) => start > end || end as usize >= scene.phases.len(),
                    _ => true,
                })
            || !self.scenes.insert(id)
        {
            return false;
        }
        self.scene_runs.insert(id, super::SceneRunState::default());
        let object = self.object_of(Handle::Form(id), alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnBegin".into(),
            args: Vec::new(),
        });
        true
    }

    /// `Scene.Stop`: not playing, and `OnEnd` to the scene's scripts.
    pub fn stop_scene(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        if !self.scenes.remove(&id) {
            return false;
        }
        if let Some(run) = self.scene_runs.remove(&id) {
            for (actor, previous) in run.prior_packages {
                crate::ai::restore_scene_package(
                    self,
                    actor,
                    previous,
                    run.prior_aliases.get(&actor).copied().flatten(),
                    alloc,
                );
            }
        }
        let object = self.object_of(Handle::Form(id), alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnEnd".into(),
            args: Vec::new(),
        });
        true
    }

    /// Current zero-based phase for a playing scene.
    pub fn scene_phase(&self, id: GlobalFormId) -> Option<u32> {
        self.scene_runs.get(&id).map(|run| run.phase)
    }

    /// Whether a valid SCEN action slot has completed. A valid action on a
    /// stopped scene is not complete.
    pub fn scene_action_complete(&self, id: GlobalFormId, action_slot: u32) -> Option<bool> {
        let record = self.record(id)?;
        let scene = Scene::from_record(record).ok()?;
        let slot = usize::try_from(action_slot).ok()?;
        scene.actions.get(slot)?;
        Some(
            self.scene_runs
                .get(&id)
                .is_some_and(|run| run.completed_actions.contains(&action_slot)),
        )
    }

    /// Mark an action slot complete when it ends in the current phase.
    /// `action_slot` is its zero-based position in the SCEN Actions list.
    pub fn complete_scene_action(&mut self, id: GlobalFormId, action_slot: u32) -> bool {
        self.complete_scene_action_inner(id, action_slot, false)
    }

    fn complete_scene_action_inner(
        &mut self,
        id: GlobalFormId,
        action_slot: u32,
        timer_expired: bool,
    ) -> bool {
        let Some(run) = self.scene_runs.get(&id) else {
            return false;
        };
        let phase = run.phase;
        let Some(record) = self.record(id) else {
            return false;
        };
        let Ok(scene) = Scene::from_record(record) else {
            return false;
        };
        if scene
            .actions
            .get(action_slot as usize)
            .is_none_or(|action| {
                (matches!(action.data, SceneActionData::Timer { .. }) && !timer_expired)
                    || action.end_phase.is_none_or(|end| end < phase)
                    || action.start_phase.is_some_and(|start| start > phase)
            })
        {
            return false;
        }
        self.scene_runs
            .get_mut(&id)
            .is_some_and(|run| run.completed_actions.insert(action_slot))
    }

    /// Advance once every action ending in the current phase has completed.
    /// A scene with no remaining phase ends and raises `OnEnd`.
    pub fn advance_scene_phase(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        let Some(run) = self.scene_runs.get(&id) else {
            return false;
        };
        let phase = run.phase;
        let completed = &run.completed_actions;
        let Some(record) = self.record(id) else {
            return false;
        };
        let Ok(scene) = Scene::from_record(record) else {
            return false;
        };
        let required: Vec<u32> = scene
            .actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                action.end_phase == Some(phase)
                    && action.start_phase.is_none_or(|start| start <= phase)
            })
            .map(|(slot, _)| slot as u32)
            .collect();
        if !required.iter().all(|slot| completed.contains(slot)) {
            return false;
        }
        let mut ending = Vec::new();
        for (slot, action) in scene.actions.iter().enumerate() {
            let slot = slot as u32;
            if action.end_phase != Some(phase)
                || !matches!(action.data, SceneActionData::Package { .. })
                || !self
                    .scene_runs
                    .get(&id)
                    .is_some_and(|run| run.started_actions.contains(&slot))
            {
                continue;
            }
            let Some(actor_id) = action.alias_id.filter(|alias| *alias >= 0) else {
                continue;
            };
            let Some(quest) = scene.quest.and_then(|raw| self.resolve_from(id, raw)) else {
                continue;
            };
            let Some(actor) = self.alias_ref(quest, actor_id as u32) else {
                continue;
            };
            let previous = self
                .scene_runs
                .get(&id)
                .and_then(|run| run.prior_packages.get(&actor))
                .copied()
                .flatten();
            let previous_alias = self
                .scene_runs
                .get(&id)
                .and_then(|run| run.prior_aliases.get(&actor))
                .copied()
                .flatten();
            ending.push((slot, actor, previous, previous_alias));
        }
        for (slot, actor, previous, previous_alias) in ending {
            crate::ai::restore_scene_package(self, actor, previous, previous_alias, alloc);
            if let Some(run) = self.scene_runs.get_mut(&id) {
                run.prior_packages.remove(&actor);
                run.prior_aliases.remove(&actor);
                run.started_actions.remove(&slot);
            }
        }
        let next = phase + 1;
        if next as usize >= scene.phases.len() {
            return self.stop_scene(id, alloc);
        }
        if let Some(run) = self.scene_runs.get_mut(&id) {
            run.phase = next;
            run.completed_actions.retain(|slot| {
                scene
                    .actions
                    .get(*slot as usize)
                    .is_some_and(|action| action.end_phase.is_some_and(|end| end >= next))
            });
            run.started_actions.retain(|slot| {
                scene
                    .actions
                    .get(*slot as usize)
                    .is_some_and(|action| action.end_phase.is_some_and(|end| end > next))
            });
            true
        } else {
            false
        }
    }

    /// Start package and timer actions whose start phase is current, and
    /// finish timers once their real-time duration expires.
    pub fn tick_scene_actions(&mut self, alloc: Alloc<'_>) {
        let scenes: Vec<_> = self.scene_runs.keys().copied().collect();
        for id in scenes {
            let Some(record) = self.record(id) else {
                continue;
            };
            let Ok(scene) = Scene::from_record(record) else {
                continue;
            };
            let phase = self.scene_phase(id).unwrap_or_default();
            for (slot, action) in scene.actions.iter().enumerate() {
                let slot = slot as u32;
                let started = self
                    .scene_runs
                    .get(&id)
                    .is_some_and(|run| run.started_actions.contains(&slot));
                if action.start_phase == Some(phase) && !started {
                    match &action.data {
                        SceneActionData::Package { packages } => {
                            let actor =
                                action
                                    .alias_id
                                    .filter(|alias| *alias >= 0)
                                    .and_then(|alias| {
                                        let quest = scene
                                            .quest
                                            .and_then(|raw| self.resolve_from(id, raw))?;
                                        self.alias_ref(quest, alias as u32)
                                    });
                            let package =
                                packages.first().and_then(|raw| self.resolve_from(id, *raw));
                            if let (Some(actor), Some(package)) = (actor, package) {
                                let previous = self
                                    .actor(actor)
                                    .map(|state| (state.ai.package, state.ai.alias));
                                if let Some(run) = self.scene_runs.get_mut(&id) {
                                    run.prior_packages
                                        .entry(actor)
                                        .or_insert(previous.and_then(|prior| prior.0));
                                    run.prior_aliases
                                        .entry(actor)
                                        .or_insert(previous.and_then(|prior| prior.1));
                                    run.started_actions.insert(slot);
                                }
                                if let Some(state) = self.actor(actor) {
                                    state.ai.scene_package = Some(package);
                                    state.ai.reevaluate = true;
                                    self.evaluate_package(actor, alloc);
                                }
                            }
                        }
                        SceneActionData::Timer {
                            duration: Some(duration),
                        } if duration.is_finite() && *duration >= 0.0 => {
                            if let Some(run) = self.scene_runs.get_mut(&id) {
                                run.started_actions.insert(slot);
                                run.timer_deadlines
                                    .insert(slot, self.clock.real_seconds + f64::from(*duration));
                            }
                        }
                        _ => {}
                    }
                }
            }
            let due: Vec<_> = self.scene_runs.get(&id).map_or_else(Vec::new, |run| {
                run.timer_deadlines
                    .iter()
                    .filter_map(|(slot, due)| (self.clock.real_seconds >= *due).then_some(*slot))
                    .collect()
            });
            for slot in due {
                if self.complete_scene_action_inner(id, slot, true) {
                    if let Some(run) = self.scene_runs.get_mut(&id) {
                        run.timer_deadlines.remove(&slot);
                        run.started_actions.remove(&slot);
                    }
                }
            }
            let package_finishes: Vec<_> = scene
                .actions
                .iter()
                .enumerate()
                .filter_map(|(slot, action)| {
                    (action.end_phase == Some(phase)
                        && matches!(action.data, SceneActionData::Package { .. })
                        && self
                            .scene_runs
                            .get(&id)
                            .is_some_and(|run| run.started_actions.contains(&(slot as u32))))
                    .then_some(slot as u32)
                })
                .collect();
            for slot in package_finishes {
                self.complete_scene_action(id, slot);
            }
        }
    }

    /// `Sound.Play`: a fresh instance id, never zero.
    pub fn play_sound(&mut self, sound: GlobalFormId, source: GlobalFormId) -> i32 {
        self.next_sound += 1;
        self.sound_instances
            .insert(self.next_sound, (sound, source, 1.0));
        self.next_sound
    }
}
