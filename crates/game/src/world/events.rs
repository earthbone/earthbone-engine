// SPDX-License-Identifier: GPL-3.0-or-later

//! Events natives raise for registered listeners: tracked stats, sleep,
//! animation events, line of sight, scenes and sound instances. Each one
//! queues into `pending` for the VM to deliver.

use soe_esm::records::gameplay::Scene;
use soe_esm::GlobalFormId;
use soe_papyrus::{ObjectId, Value};

use super::{Alloc, Handle, Pending, World};

/// Authored membrane settings used by the host when displaying an EffectShader.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectShaderAppearance {
    /// EFSH `ICON`, the membrane fill texture path.
    pub fill_texture: Option<String>,
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
        let fill_texture = record.find(super::T(b"ICON")).and_then(|bytes| {
            let path = bytes.split(|byte| *byte == 0).next().unwrap_or_default();
            let path = String::from_utf8_lossy(path).trim().to_string();
            (!path.is_empty()).then_some(path)
        });
        let Some(data) = record.find(super::T(b"DATA")) else {
            return EffectShaderAppearance {
                fill_texture,
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
