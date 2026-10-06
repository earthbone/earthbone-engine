// SPDX-License-Identifier: GPL-3.0-or-later

//! Main-thread frame publication for Godot 4.4. Monitor names, callable
//! arguments and removal follow godot-docs/classes/class_performance.rst.

use godot::classes::Performance;
use godot::prelude::*;
use soe_trace::System;

use super::PlayerNode;

fn health_dictionary(health: &soe_trace::PapyrusHealth) -> VarDictionary {
    let mut events = VarDictionary::new();
    for (name, event) in &health.events {
        events.set(
            name.as_str(),
            &vdict! {
                "queued" => event.queued as i64,
                "peak_per_second" => event.peak_per_second as i64,
            },
        );
    }
    vdict! {
        "backlog" => health.backlog as i64,
        "oldest_wait_seconds" => health.oldest_wait_seconds,
        "repeating_updates" => health.updates.repeating as i64,
        "single_updates" => health.updates.single as i64,
        "repeating_game_time_updates" => health.updates.repeating_game_time as i64,
        "single_game_time_updates" => health.updates.single_game_time as i64,
        "waiting_seconds" => health.waiting_seconds,
        "running_ms" => health.running_ns as f64 / 1e6,
        "suspended_stacks" => health.suspended_stacks as i64,
        "events" => &events,
    }
}

impl PlayerNode {
    pub(super) fn register_system_monitors(&mut self) {
        let mut performance = Performance::singleton();
        let category = if System::ALL.iter().any(|system| {
            performance.has_custom_monitor(&StringName::from(
                format!("SOE/{}_ms", system.name()).as_str(),
            ))
        }) {
            format!("SOE_{}", self.base().instance_id().to_i64())
        } else {
            "SOE".to_owned()
        };
        for (index, system) in System::ALL.into_iter().enumerate() {
            let name = StringName::from(format!("{category}/{}_ms", system.name()).as_str());
            let callback = self
                .base()
                .callable("system_monitor_ms")
                .bindv(&varray![index as i64]);
            performance.add_custom_monitor(&name, &callback);
            self.monitor_names.push(name);
        }
    }

    pub(super) fn remove_system_monitors(&mut self) {
        let mut performance = Performance::singleton();
        for name in self.monitor_names.drain(..) {
            performance.remove_custom_monitor(&name);
        }
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Complete keys only after the physics step has applied their controls.
    #[func]
    fn profile_frame_presented(&mut self) {
        for latency in self.profile_applied_inputs.drain(..) {
            latency.presented();
        }
    }

    /// Enable optional per-entity profiling from the F4 overlay.
    #[func]
    fn set_entity_profiling(&mut self, enabled: bool) {
        if enabled {
            soe_trace::Profile::enable();
        } else {
            soe_trace::Profile::disable();
        }
    }

    /// Whether entity capture is currently enabled.
    #[func]
    fn entity_profiling_enabled(&self) -> bool {
        soe_trace::Profile::enabled()
    }

    /// Top actors, scripts, quests and plugins over the preceding second.
    #[func]
    fn entity_profile_overlay(&self) -> GString {
        GString::from(soe_trace::Profile::overlay_text().as_str())
    }

    /// Export the captured scopes and session entity counters for Perfetto.
    #[func]
    #[allow(clippy::needless_pass_by_value)] // CONTEXT: Godot callbacks receive owned Variants.
    fn write_entity_profile(&self, directory: GString) -> bool {
        let directory = std::path::PathBuf::from(directory.to_string());
        match soe_trace::Profile::export_chrome_json(&directory.join("soe-profile.json"))
            .and_then(|()| soe_trace::Profile::export_summary(&directory.join("soe-entities.json")))
        {
            Ok(()) => true,
            Err(error) => {
                godot_error!("Could not write entity profile: {error}");
                false
            }
        }
    }

    /// Frame Papyrus health with per-script and per-attachment-plugin rows.
    #[func]
    fn papyrus_health(&self) -> VarDictionary {
        let report = self
            .game
            .as_ref()
            .map(soe_game::Game::papyrus_health)
            .unwrap_or_default();
        let mut scripts = VarDictionary::new();
        for (script, health) in &report.scripts {
            scripts.set(script.as_str(), &health_dictionary(health));
        }
        let mut plugins = VarDictionary::new();
        for (plugin, rows) in &report.plugins {
            let mut scripts = VarDictionary::new();
            for (script, health) in rows {
                scripts.set(script.as_str(), &health_dictionary(health));
            }
            plugins.set(plugin.as_str(), &scripts);
        }
        vdict! { "total" => &health_dictionary(&report.total), "scripts" => &scripts, "plugins" => &plugins }
    }

    /// Runtime progress and cached navigation for fixed-step acceptance checks.
    #[func]
    fn budget_runtime_status(&self) -> VarDictionary {
        let Some(game) = self.game.as_ref() else {
            return VarDictionary::new();
        };
        let world = game.world();
        let triangles = world.ai.nav_graph().map_or(0, |graph| {
            graph
                .meshes()
                .iter()
                .map(|mesh| mesh.triangles.len())
                .sum::<usize>()
        });
        let world_editor_id = self
            .data_source
            .as_ref()
            .and_then(|source| {
                let cell = self.loaded_cell?;
                let worldspace = source.exterior_world(cell)?;
                source.order().winner(worldspace)?.editor_id()
            })
            .unwrap_or_default();
        vdict! {
            "ticks" => game.frame_history().frames() as i64,
            "seconds" => world.clock.real_seconds,
            "evaluations" => world.ai.evaluations as i64,
            "navmesh_triangles" => triangles as i64,
            "world_editor_id" => world_editor_id.as_str(),
        }
    }

    /// Open a fixed-step CPU timing window. Nested windows are rejected.
    #[func]
    fn begin_budget_frame(&mut self) -> bool {
        if self.budget_frame.is_some() {
            return false;
        }
        self.budget_frame = Some(soe_trace::FrameSnapshot::now());
        true
    }

    /// Close the CPU window before pacing or telemetry. Empty without a begin.
    #[func]
    fn end_budget_frame(&mut self) -> VarDictionary {
        let Some(frame) = self.budget_frame.take() else {
            return VarDictionary::new();
        };
        let sample = frame.finish();
        let mut systems = VarDictionary::new();
        for system in System::ALL {
            systems.set(system.name(), sample.milliseconds(system));
        }
        vdict! {
            "frame_ms" => sample.frame_ns as f64 / 1e6,
            "systems" => &systems,
        }
    }

    /// Milliseconds in the latest presentation frame, for custom monitor callbacks.
    #[func]
    fn system_monitor_ms(&self, index: i64) -> f64 {
        usize::try_from(index)
            .ok()
            .and_then(|index| System::ALL.get(index))
            .map_or(0.0, |system| self.host_history.last().milliseconds(*system))
    }

    /// Last host frame's exclusive timings, percentages and rolling percentiles.
    #[func]
    fn system_frame_breakdown(&self) -> VarDictionary {
        let mut report = VarDictionary::new();
        for system in System::ALL {
            let stats = self.host_history.statistics(system);
            report.set(
                system.name(),
                &vdict! {
                    "live_bytes" => self.host_history.last().live_bytes[system as usize] as i64,
                    "allocations" => self.host_history.last().allocation_calls[system as usize] as i64,
                    "ms" => self.host_history.last().milliseconds(system),
                    "percent" => self.host_history.last().percent(system),
                    "p50_ms" => stats.p50_ns as f64 / 1e6,
                    "p99_ms" => stats.p99_ns as f64 / 1e6,
                    "worst_ms" => stats.worst_ns as f64 / 1e6,
                },
            );
        }
        report
    }

    /// All session totals and worst times with rolling p50 and p99 per system.
    #[func]
    fn system_summary(&self) -> GString {
        GString::from(self.host_history.summary_text().as_str())
    }
}
