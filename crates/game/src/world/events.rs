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

use super::{
    Alloc, AnimationCompletion, AnimationHostNotice, AnimationRequest, AnimationWait, Handle,
    Pending, World,
};

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

/// Authored membrane settings used by the host when displaying an EffectShader.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectShaderAppearance {
    /// EFSH `ICON`, the membrane fill texture path.
    pub fill_texture: Option<String>,
    /// EFSH `ICO2`, the membrane loop texture path.
    pub loop_texture: Option<String>,
    /// EFSH `NAM7`, the membrane post texture path.
    pub end_texture: Option<String>,
    /// EFSH `DATA` color key one in RGBA byte order.
    pub color: [u8; 4],
    /// EFSH `DATA` source and destination blend values.
    pub blend: (u32, u32),
    /// EFSH `DATA` membrane alpha timing, in seconds.
    pub fade: [f32; 4],
}

impl Default for EffectShaderAppearance {
    fn default() -> Self {
        Self {
            fill_texture: None,
            loop_texture: None,
            end_texture: None,
            color: [255, 255, 255, 255],
            blend: (6, 7),
            fade: [0.0; 4],
        }
    }
}

/// A transient request for the host to display one EffectShader on a reference.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectShaderRequest {
    pub shader: GlobalFormId,
    pub target: GlobalFormId,
    pub duration: f32,
    pub playing: bool,
    pub appearance: EffectShaderAppearance,
}

/// A transient physics operation requested by a Papyrus reference native.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReferencePhysicsRequest {
    /// Set the host motion mode for a reference with a resident movable body.
    MotionType {
        reference: GlobalFormId,
        motion_type: u32,
        allow_activate: bool,
    },
    /// Apply a normalized Skyrim-space direction scaled by `magnitude`.
    Impulse {
        reference: GlobalFormId,
        direction: [f32; 3],
        magnitude: f32,
    },
}

impl World {
    /// Queue a physics operation for the host to apply to a resident body.
    pub fn request_reference_physics(&mut self, request: ReferencePhysicsRequest) {
        self.reference_physics_requests.push(request);
    }

    /// Drain queued reference physics operations after host collision sync.
    pub fn take_reference_physics_requests(&mut self) -> Vec<ReferencePhysicsRequest> {
        std::mem::take(&mut self.reference_physics_requests)
    }

    /// Queue a graph event for the exact placed reference that owns it.
    pub fn request_animation(&mut self, target: GlobalFormId, animation: &str) {
        self.animation_requests.push(AnimationRequest {
            target,
            animation: animation.to_string(),
        });
    }

    /// Queue a graph event emitted by a host visual for script delivery.
    pub fn queue_animation_report(&mut self, source: GlobalFormId, name: &str) {
        self.animation_host_notices
            .push(AnimationHostNotice::Event {
                source,
                name: name.to_string(),
            });
    }

    /// Drain ordered graph and unload notifications for the game loop.
    pub fn take_animation_host_notices(&mut self) -> Vec<AnimationHostNotice> {
        std::mem::take(&mut self.animation_host_notices)
    }

    /// Queue a host visual unload so latent waits can return false.
    pub fn queue_animation_unload(&mut self, target: GlobalFormId) {
        self.animation_host_notices
            .push(AnimationHostNotice::Unloaded { target });
    }

    /// Record a latent wait for an event from one reference's graph.
    pub(crate) fn wait_for_animation(
        &mut self,
        target: GlobalFormId,
        event: &str,
        latent: LatentId,
    ) {
        self.animation_waits.push(AnimationWait {
            target,
            event: event.to_string(),
            latent,
        });
    }

    /// A host graph event reached its target. Matching waits are removed
    /// before completion is queued, so duplicate reports cannot resolve twice.
    pub fn animation_graph_event(&mut self, source: GlobalFormId, name: &str, alloc: Alloc<'_>) {
        self.animation_event(source, name, alloc);
    }

    /// Drain latent results for the game loop to resolve.
    pub fn take_animation_completions(&mut self) -> Vec<AnimationCompletion> {
        std::mem::take(&mut self.animation_completions)
    }

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
            appearance: self.effect_shader_appearance(shader),
        });
    }

    /// Read the membrane settings supported by the host from the winning EFSH.
    pub fn effect_shader_appearance(&self, shader: GlobalFormId) -> EffectShaderAppearance {
        let Some(record) = self
            .record(shader)
            .filter(|record| record.tag == super::T(b"EFSH"))
        else {
            return EffectShaderAppearance::default();
        };
        let read_texture = |tag| {
            record.find(super::T(tag)).and_then(|bytes| {
                let path = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
                let path = String::from_utf8_lossy(path).trim().to_string();
                (!path.is_empty()).then_some(path)
            })
        };
        let fill_texture = read_texture(b"ICON");
        let loop_texture = read_texture(b"ICO2");
        let end_texture = read_texture(b"NAM7");
        let Some(data) = record.find(super::T(b"DATA")) else {
            return EffectShaderAppearance {
                fill_texture,
                loop_texture,
                end_texture,
                ..EffectShaderAppearance::default()
            };
        };
        let mut color = EffectShaderAppearance::default().color;
        if data.len() >= 20 {
            color.copy_from_slice(&data[16..20]);
        }
        let blend = if data.len() >= 96 {
            (read_u32(data, 4), read_u32(data, 92))
        } else {
            EffectShaderAppearance::default().blend
        };
        EffectShaderAppearance {
            fill_texture,
            loop_texture,
            end_texture,
            color,
            blend,
            fade: [20, 24, 28, 32].map(|offset| read_f32(data, offset)),
        }
    }

    /// Drain visual requests after the host has reconciled them.
    pub fn take_effect_shader_requests(&mut self) -> Vec<EffectShaderRequest> {
        std::mem::take(&mut self.effect_shader_requests)
    }

    /// Take queued Story Manager inputs for the scheduler.
    pub fn take_story_manager_events(&mut self) -> Vec<crate::quest::state::StoryManagerEvent> {
        std::mem::take(&mut self.story_manager_events)
    }

    /// Queue a help notification once for an input event until it is reset.
    pub fn show_help_message(&mut self, event: &str, message: String) {
        if !event.is_empty() && self.help_message_events.insert(event.to_string()) {
            self.notifications.push(message);
        }
    }

    /// Allow a help notification for `event` to be shown again.
    pub fn reset_help_message(&mut self, event: &str) {
        if !event.is_empty() {
            self.help_message_events.remove(event);
        }
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

        let mut index = 0;
        while index < self.animation_waits.len() {
            let wait = &self.animation_waits[index];
            let matches_event = match event {
                None => true,
                Some(name) => wait.event.eq_ignore_ascii_case(name),
            };
            if wait.target == source && matches_event {
                let wait = self.animation_waits.remove(index);
                self.animation_completions.push(AnimationCompletion {
                    latent: wait.latent,
                    succeeded: event.is_some(),
                });
            } else {
                index += 1;
            }
        }
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
    pub fn scene_action_slot_complete(&self, id: GlobalFormId, action_slot: u32) -> Option<bool> {
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

    /// Return whether the action with the Papyrus action ID has completed.
    ///
    /// # Errors
    ///
    /// This query has no error result. Invalid, unknown and nonplaying actions
    /// return `false`.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    #[must_use]
    pub fn scene_action_complete(&self, id: GlobalFormId, action_id: i32) -> bool {
        let Ok(action_id) = u32::try_from(action_id) else {
            return false;
        };
        let Some(run) = self.scene_runs.get(&id) else {
            return false;
        };
        let Some(record) = self.record(id) else {
            return false;
        };
        let Ok(scene) = Scene::from_record(record) else {
            return false;
        };
        scene.actions.iter().enumerate().any(|(slot, action)| {
            action.index == Some(action_id) && run.completed_actions.contains(&(slot as u32))
        })
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

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    let Some(value) = bytes.get(offset..offset.saturating_add(4)) else {
        return 0;
    };
    u32::from_le_bytes([value[0], value[1], value[2], value[3]])
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    let Some(value) = bytes.get(offset..offset.saturating_add(4)) else {
        return 0.0;
    };
    f32::from_le_bytes([value[0], value[1], value[2], value[3]])
}

#[cfg(test)]
mod hst_10_tests {
    use super::*;
    use crate::world::World;
    use soe_esm::LoadOrder;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    #[test]
    fn hst_10_matching_reference_event_completes_wait_once() {
        let mut world = World::new(LoadOrder::new());
        world.wait_for_animation(id(1), "Done", LatentId(4));
        world.wait_for_animation(id(2), "Done", LatentId(5));

        world.animation_graph_event(id(2), "Done", &mut |_, _| unreachable!());
        assert_eq!(
            world.take_animation_completions(),
            [AnimationCompletion {
                latent: LatentId(5),
                succeeded: true,
            }]
        );
        world.animation_graph_event(id(1), "Done", &mut |_, _| unreachable!());
        assert_eq!(
            world.take_animation_completions(),
            [AnimationCompletion {
                latent: LatentId(4),
                succeeded: true,
            }]
        );
        world.animation_graph_event(id(1), "Done", &mut |_, _| unreachable!());
        assert!(world.take_animation_completions().is_empty());
    }

    #[test]
    fn hst_10_unloaded_reference_fails_all_waits_once() {
        let mut world = World::new(LoadOrder::new());
        world.wait_for_animation(id(1), "DoneA", LatentId(6));
        world.wait_for_animation(id(1), "DoneB", LatentId(7));
        world.animation_graph_unloaded(id(1));
        assert_eq!(
            world.take_animation_completions(),
            [
                AnimationCompletion {
                    latent: LatentId(6),
                    succeeded: false,
                },
                AnimationCompletion {
                    latent: LatentId(7),
                    succeeded: false,
                },
            ]
        );
        world.animation_graph_unloaded(id(1));
        assert!(world.take_animation_completions().is_empty());
    }

    #[test]
    fn hst_10_unload_before_end_event_reports_failure() {
        let mut world = World::new(LoadOrder::new());
        world.wait_for_animation(id(1), "Done", LatentId(8));
        world.queue_animation_unload(id(1));
        world.queue_animation_report(id(1), "Done");

        for notice in world.take_animation_host_notices() {
            match notice {
                AnimationHostNotice::Event { source, name } => {
                    world.animation_graph_event(source, &name, &mut |_, _| unreachable!());
                }
                AnimationHostNotice::Unloaded { target } => {
                    world.animation_graph_unloaded(target);
                }
            }
        }

        assert_eq!(
            world.take_animation_completions(),
            [AnimationCompletion {
                latent: LatentId(8),
                succeeded: false,
            }]
        );
    }
}
