// SPDX-License-Identifier: GPL-3.0-or-later

//! Main-thread frame publication for Godot 4.4. Monitor names, callable
//! arguments and removal follow godot-docs/classes/class_performance.rst.

use godot::classes::Performance;
use godot::prelude::*;
use soe_game::ai::metrics::{ProcessLevel, Stimulus, DECISION_PHASES, PROCEDURE_PHASES};
use soe_trace::System;

use super::PlayerNode;

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

    /// Always-on inclusive AI phases, process counts and per-actor path/reaction data.
    #[func]
    fn ai_breakdown(&self) -> VarDictionary {
        let mut phases = VarDictionary::new();
        let mut procedures = VarDictionary::new();
        let mut levels = VarDictionary::new();
        let mut paths = VarDictionary::new();
        let mut reactions = VarDictionary::new();
        let mut stuck = PackedStringArray::new();
        let world = self.game.as_ref().map(soe_game::Game::world);
        for phase in DECISION_PHASES.into_iter().chain(PROCEDURE_PHASES) {
            let duration = world
                .as_ref()
                .and_then(|world| world.ai.planning_phases.get(phase))
                .copied()
                .unwrap_or_default();
            let calls = world
                .as_ref()
                .and_then(|world| world.ai.phase_calls.get(phase))
                .copied()
                .unwrap_or_default();
            let row = vdict! { "us" => duration.as_secs_f64() * 1e6, "calls" => calls as i64 };
            if let Some(name) = phase.strip_prefix("procedure.") {
                procedures.set(name, &row);
            } else {
                phases.set(phase, &row);
            }
        }
        if let Some(world) = &world {
            for (phase, duration) in &world.ai.planning_phases {
                if !phase.starts_with("procedure.") {
                    let calls = world.ai.phase_calls.get(phase).copied().unwrap_or_default();
                    phases.set(
                        *phase,
                        &vdict! { "us" => duration.as_secs_f64() * 1e6, "calls" => calls as i64 },
                    );
                }
            }
        }
        for level in ProcessLevel::ALL {
            let metrics = world.as_ref().map_or_else(Default::default, |world| {
                world.ai.metrics.levels[level as usize].clone()
            });
            levels.set(
                level.name(),
                &vdict! {
                    "actors" => metrics.actors as i64, "updates" => metrics.updates as i64,
                    "us" => metrics.time.as_secs_f64() * 1e6,
                    "interval_seconds" => metrics.mean_interval_seconds,
                },
            );
        }
        if let Some(world) = &world {
            let now = std::time::Instant::now();
            for (actor, metrics) in world.ai.path_metrics() {
                let (requests, replans) = metrics.rates(now);
                let mut samples = Array::<VarDictionary>::new();
                for sample in &metrics.samples {
                    samples.push(&vdict! {
                        "latency_us" => sample.ready.duration_since(sample.started).as_secs_f64() * 1e6,
                        "nodes_expanded" => sample.expanded as i64, "length" => sample.length,
                        "failed" => sample.failed, "cancelled" => sample.cancelled, "replan" => sample.replan,
                    });
                }
                paths.set(actor.to_string(), &vdict! {
                    "requests" => metrics.requests as i64, "completed" => metrics.completed as i64,
                    "failures" => metrics.failures as i64, "cancellations" => metrics.cancellations as i64,
                    "replans" => metrics.replans as i64, "nodes_expanded" => metrics.expanded as i64,
                    "requests_per_second" => requests as i64, "replans_per_second" => replans as i64,
                    "samples" => &samples,
                });
            }
            for (actor, metrics) in &world.ai.metrics.actors {
                let mut kinds = VarDictionary::new();
                for stimulus in Stimulus::ALL {
                    let row = &metrics.reactions[stimulus as usize];
                    kinds.set(stimulus.name(), &vdict! {
                        "responses" => row.responses as i64, "pending" => row.pending.is_some(),
                        "last_us" => row.last.as_secs_f64() * 1e6, "worst_us" => row.worst.as_secs_f64() * 1e6,
                        "total_us" => row.total.as_secs_f64() * 1e6,
                        "response" => row.response.map_or("none".to_owned(), |response| format!("{response:?}")),
                    });
                }
                reactions.set(actor.to_string(), &kinds);
                if metrics.stuck {
                    stuck.push(&GString::from(actor.to_string().as_str()));
                }
            }
        }
        vdict! {
            "phases" => &phases, "procedures" => &procedures, "levels" => &levels,
            "paths" => &paths, "reactions" => &reactions, "stuck" => &stuck,
            "capabilities" => &vdict! { "process_scheduler" => false, "combat_style_choice" => false, "idle_chatter" => false },
        }
    }

    /// All session totals and worst times with rolling p50 and p99 per system.
    #[func]
    fn system_summary(&self) -> GString {
        GString::from(
            format!(
                "{}{}",
                self.host_history.summary_text(),
                soe_trace::LoadTrace::summary_text()
            )
            .as_str(),
        )
    }
}
