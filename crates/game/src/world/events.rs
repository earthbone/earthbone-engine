// SPDX-License-Identifier: GPL-3.0-or-later

//! Events natives raise for registered listeners: tracked stats, sleep,
//! animation events, line of sight, scenes and sound instances. Each one
//! queues into `pending` for the VM to deliver.

use soe_esm::records::gameplay::Scene;
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
        self.scene_runs.remove(&id);
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

    /// Mark an action slot complete when it ends in the current phase.
    /// `action_slot` is its zero-based position in the SCEN Actions list.
    pub fn complete_scene_action(&mut self, id: GlobalFormId, action_slot: u32) -> bool {
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
                action.end_phase != Some(phase)
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
                    .is_some_and(|action| action.end_phase.is_some_and(|end| end > next))
            });
            true
        } else {
            false
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
