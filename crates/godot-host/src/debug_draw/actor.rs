// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional live behavior-clock samples. Havok clip clocks come from the shared
//! interpreter, durations from decoded HKX, host frames from Godot Engine.

use std::collections::BTreeMap;

use godot::classes::Engine;
use godot::obj::Singleton;
use soe_hkx::behavior::NodeKind;
use soe_hkx::interp::Interpreter;

use crate::actor::ActorAssets;
use crate::godot_actor::SkinnedActorNode;

#[derive(Default)]
pub(super) struct AnimationClock {
    frame: u64,
    updates: usize,
    step: f32,
    total_step: f32,
    previous: BTreeMap<usize, f32>,
    rates: BTreeMap<usize, f32>,
}

impl AnimationClock {
    pub(super) fn observe(&mut self, vm: &Interpreter<'_>, assets: &ActorAssets, delta: f32) {
        let frame = Engine::singleton().get_process_frames();
        if frame != self.frame {
            self.frame = frame;
            self.updates = 0;
            self.total_step = 0.0;
        }
        self.updates += 1;
        self.total_step += delta;
        self.step = delta;
        let mut current = BTreeMap::new();
        for (id, time) in vm.active_clips() {
            if let (Some(previous), NodeKind::Clip(generator)) =
                (self.previous.get(&id.0), &assets.graph.node(id).kind)
            {
                if delta > 0.0 {
                    let mut elapsed = time - previous;
                    if generator.mode == 1 && elapsed < 0.0 {
                        elapsed += assets
                            .clips
                            .get(&generator.animation_name)
                            .map_or(0.0, soe_hkx::interp::Clip::duration)
                            - generator.crop_start_amount_local_time
                            - generator.crop_end_amount_local_time;
                    }
                    self.rates.insert(id.0, elapsed / delta);
                }
            }
            current.insert(id.0, time);
        }
        self.previous = current;
    }
}

impl SkinnedActorNode {
    pub(crate) fn debug_nif_path(&self) -> Option<String> {
        self.assets
            .as_ref()
            .map(|assets| assets.skeleton_model_path.clone())
    }

    /// Enable sampling and report live graph state, authored and measured rates.
    /// Sampling remains active until the actor unloads, without advancing its clock.
    pub(crate) fn debug_animation_report(&mut self) -> Vec<String> {
        let (Some(assets), Some(vm)) = (self.assets.as_deref(), self.vm.as_ref()) else {
            return vec!["actor has no loaded behavior graph".into()];
        };
        let clock = self.debug_clock.get_or_insert_with(AnimationClock::default);
        let mut lines = vec![format!("graph={} graph_frame={} elapsed={:.6} host_frame={} updates_in_host_frame={} clock_step={:.6} frame_clock_sum={:.6}", assets.behavior_path, vm.frame(), self.elapsed, clock.frame, clock.updates, clock.step, clock.total_step)];
        for (id, state) in vm.active_states() {
            lines.push(format!("state {}={state}", assets.graph.node(id).name));
        }
        for (id, time) in vm.active_clips() {
            if let NodeKind::Clip(generator) = &assets.graph.node(id).kind {
                let duration = assets
                    .clips
                    .get(&generator.animation_name)
                    .map(soe_hkx::interp::Clip::duration);
                lines.push(format!("clip {} file={} time={time:.6} authored_duration={duration:?} authored_rate={} measured_playback_rate={:?} enforced_duration={} mode={} crops=({}, {})", assets.graph.node(id).name, generator.animation_name, generator.playback_speed, clock.rates.get(&id.0), generator.enforced_duration, generator.mode, generator.crop_start_amount_local_time, generator.crop_end_amount_local_time));
            }
        }
        lines
    }
}
