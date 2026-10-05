// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-19 views over the live VM, pending events and update timers. Attachment
//! ownership follows winning VMAD records, event fanout follows CK OnUpdate.

use std::collections::BTreeMap;

use soe_papyrus::ObjectId;
use soe_trace::PapyrusHealth;

use crate::{Game, Handle, Pending};

/// Read-only frame health, with script rows grouped by their attachment plugin.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PapyrusHealthReport {
    /// VM totals. Shared object timers and suspended stacks are counted once.
    pub total: PapyrusHealth,
    /// Per-script health over all attached instances and global calls.
    pub scripts: BTreeMap<String, PapyrusHealth>,
    /// Plugin, then script. Event peaks are measured across the group.
    pub plugins: BTreeMap<String, BTreeMap<String, PapyrusHealth>>,
}

impl Game {
    fn health_plugin(&self, object: Option<ObjectId>, script: &str) -> String {
        if let Some(plugin) = object.and_then(|object| self.vm.health_owner(object, script)) {
            return plugin.into();
        }
        let world = self.world.borrow();
        let owner =
            object
                .and_then(|object| world.handle_of(object))
                .and_then(|handle| match handle {
                    Handle::Form(id) => Some(
                        world
                            .reference_ref(id)
                            .and_then(|reference| reference.base)
                            .unwrap_or(id),
                    ),
                    Handle::Alias { quest, .. } => Some(quest),
                    Handle::ActiveEffect(id) => {
                        world.active_effects.get(&id).map(|effect| effect.effect)
                    }
                });
        owner
            .and_then(|owner| world.order().winner_ref(owner))
            .and_then(|record| world.order().files().get(record.file))
            .map_or_else(|| "<unattributed>".into(), |file| file.name.clone())
    }

    pub(crate) fn prepare_health_event(&mut self, object: ObjectId, event: &str) {
        for script in self.vm.event_scripts(object, event) {
            let plugin = self.health_plugin(Some(object), &script);
            self.vm.set_health_owner(object, &script, &plugin);
        }
    }

    /// Snapshot current queues, timers and stacks plus this frame's CPU and
    /// suspended durations. Queries neither pump events nor run the scheduler.
    #[must_use]
    pub fn papyrus_health(&self) -> PapyrusHealthReport {
        let mut rows: BTreeMap<_, _> = self
            .vm
            .health_rows()
            .into_iter()
            .map(|row| ((row.object, row.script), row.health))
            .collect();
        let mut report = PapyrusHealthReport {
            total: self.vm.health(),
            ..PapyrusHealthReport::default()
        };
        let world = self.world.borrow();
        for timer in &world.timers {
            let mut counts = PapyrusHealth::default();
            match (timer.interval.is_some(), timer.game_time) {
                (true, false) => counts.updates.repeating = 1,
                (false, false) => counts.updates.single = 1,
                (true, true) => counts.updates.repeating_game_time = 1,
                (false, true) => counts.updates.single_game_time = 1,
            }
            report.total.add(&counts);
            for script in self.vm.attached_scripts(timer.object) {
                rows.entry((Some(timer.object), script))
                    .or_default()
                    .add(&counts);
            }
        }
        for pending in &world.pending {
            let (object, name) = match pending {
                Pending::Event { object, name, .. } => (Some(*object), name.as_str()),
                Pending::ReferenceEvent { reference, name } => {
                    (world.object_id(Handle::Form(*reference)), name.as_str())
                }
                _ => continue,
            };
            let scripts =
                object.map_or_else(Vec::new, |object| self.vm.event_scripts(object, name));
            let name = name.to_ascii_lowercase();
            report.total.events.entry(name.clone()).or_default().queued +=
                scripts.len().max(1) as u64;
            for script in scripts {
                rows.entry((object, script))
                    .or_default()
                    .events
                    .entry(name.clone())
                    .or_default()
                    .queued += 1;
            }
        }
        drop(world);
        for ((object, script), health) in rows {
            let plugin = self.health_plugin(object, &script);
            report
                .scripts
                .entry(script.clone())
                .or_default()
                .add(&health);
            report
                .plugins
                .entry(plugin)
                .or_default()
                .entry(script)
                .or_default()
                .add(&health);
        }
        for (script, rates) in self.vm.script_event_peaks() {
            let row = report.scripts.entry(script).or_default();
            for (name, peak) in rates {
                row.events.entry(name).or_default().peak_per_second = peak;
            }
        }
        for ((plugin, script), rates) in self.vm.plugin_event_peaks() {
            let row = report
                .plugins
                .entry(plugin)
                .or_default()
                .entry(script)
                .or_default();
            for (name, peak) in rates {
                row.events.entry(name).or_default().peak_per_second = peak;
            }
        }
        report
    }
}
