// SPDX-License-Identifier: GPL-3.0-or-later

//! Main-thread frame publication for Godot 4.4. Monitor names, callable
//! arguments and removal follow godot-docs/classes/class_performance.rst.

use godot::classes::Performance;
use godot::prelude::*;
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

    /// All session totals and worst times with rolling p50 and p99 per system.
    #[func]
    fn system_summary(&self) -> GString {
        GString::from(self.host_history.summary_text().as_str())
    }

    /// Last presentation frame's physics steps, catch-up and timed queries.
    #[func]
    fn physics_frame_breakdown(&self) -> VarDictionary {
        let mut queries = VarDictionary::new();
        for (name, kind) in [
            ("raycast", soe_trace::PhysicsQueryKind::Raycast),
            ("shape_cast", soe_trace::PhysicsQueryKind::ShapeCast),
            ("overlap", soe_trace::PhysicsQueryKind::Overlap),
        ] {
            let mut by_system = VarDictionary::new();
            for system in System::ALL {
                by_system.set(
                    system.name(),
                    &vdict! {
                        "count" => self.last_physics_queries.count(kind, system) as i64,
                        "ms" => self.last_physics_queries.elapsed_ns(kind, system) as f64 / 1e6,
                    },
                );
            }
            queries.set(name, &by_system);
        }
        let mut report = vdict! {
            "steps" => self.last_physics_steps.max(self.physics_advances_since_process) as i64,
            "catchup_steps" => self.last_catchup_steps as i64,
            "jolt_step_ms" => godot::classes::Performance::singleton().get_monitor(godot::classes::performance::Monitor::TIME_PHYSICS_PROCESS) * 1000.0,
            "active_bodies" => godot::classes::Performance::singleton().get_monitor(godot::classes::performance::Monitor::PHYSICS_3D_ACTIVE_OBJECTS) as i64,
            "collision_pairs" => godot::classes::Performance::singleton().get_monitor(godot::classes::performance::Monitor::PHYSICS_3D_COLLISION_PAIRS) as i64,
            "islands" => godot::classes::Performance::singleton().get_monitor(godot::classes::performance::Monitor::PHYSICS_3D_ISLAND_COUNT) as i64,
        };
        report.set("queries", &queries);
        report
    }
}
