// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-19 transient scheduler diagnostics. CK Utility_Script and OnUpdate
//! define wait domains and event fanout. Counts never alter scheduling or saves.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use soe_trace::{EventRate, PapyrusHealth};

use super::{Frame, ObjectId, ThreadId, ThreadState, Vm};

type Key = (Option<ObjectId>, String);

/// Health of a defining script on one object, or of a global script call.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptHealth {
    /// None for global calls.
    pub object: Option<ObjectId>,
    /// Case-normalized defining script name.
    pub script: String,
    /// Live counts, frame durations and session event peaks.
    pub health: PapyrusHealth,
}

#[derive(Default)]
pub(super) struct Tracker {
    deferred: HashMap<ThreadId, f64>,
    queued: HashMap<ThreadId, (Key, String)>,
    frame: BTreeMap<Key, PapyrusHealth>,
    running: HashMap<Option<ObjectId>, HashMap<String, u64>>,
    rates: BTreeMap<Key, BTreeMap<String, EventRate>>,
    total_rates: BTreeMap<String, EventRate>,
    owners: BTreeMap<Key, String>,
    script_rates: BTreeMap<String, BTreeMap<String, EventRate>>,
    plugin_rates: BTreeMap<(String, String), BTreeMap<String, EventRate>>,
}

fn key(frame: &Frame) -> Key {
    (
        frame.self_object,
        frame
            .script
            .as_ref()
            .map_or_else(|| "<native>".into(), |s| s.name.to_ascii_lowercase()),
    )
}

fn suspended(state: &ThreadState) -> bool {
    !matches!(state, ThreadState::Runnable | ThreadState::Done)
}

impl Vm {
    /// Associate a diagnostic row with the plugin supplying its script attachment.
    /// This metadata is transient and does not affect form or script lookup.
    pub fn set_health_owner(&mut self, object: ObjectId, script: &str, plugin: &str) {
        self.health
            .owners
            .insert((Some(object), script.to_ascii_lowercase()), plugin.into());
    }

    /// Plugin recorded for a script attachment, when the host supplied one.
    #[must_use]
    pub fn health_owner(&self, object: ObjectId, script: &str) -> Option<&str> {
        if let Some(plugin) = self
            .health
            .owners
            .get(&(Some(object), script.to_ascii_lowercase()))
        {
            return Some(plugin);
        }
        self.objects.get(&object)?.instances.iter().find_map(|id| {
            let instance = self.instances.get(id.0)?;
            if !instance
                .chain
                .iter()
                .any(|parent| parent.name.eq_ignore_ascii_case(script))
            {
                return None;
            }
            let root = instance.chain.first()?;
            self.health
                .owners
                .get(&(Some(object), root.name.to_ascii_lowercase()))
                .map(String::as_str)
        })
    }

    /// Exact session event peaks across all instances of each script.
    #[must_use]
    pub fn script_event_peaks(&self) -> BTreeMap<String, BTreeMap<String, u64>> {
        self.health
            .script_rates
            .iter()
            .map(|(script, rates)| {
                (
                    script.clone(),
                    rates
                        .iter()
                        .map(|(name, rate)| (name.clone(), rate.peak()))
                        .collect(),
                )
            })
            .collect()
    }

    /// Exact session event peaks per attachment plugin and defining script.
    #[must_use]
    pub fn plugin_event_peaks(&self) -> BTreeMap<(String, String), BTreeMap<String, u64>> {
        self.health
            .plugin_rates
            .iter()
            .map(|(group, rates)| {
                (
                    group.clone(),
                    rates
                        .iter()
                        .map(|(name, rate)| (name.clone(), rate.peak()))
                        .collect(),
                )
            })
            .collect()
    }

    /// Names of scripts attached to this object, in attachment order.
    #[must_use]
    pub fn attached_scripts(&self, object: ObjectId) -> Vec<String> {
        self.objects
            .get(&object)
            .into_iter()
            .flat_map(|object| &object.instances)
            .filter_map(|id| self.instances.get(id.0))
            .filter_map(|instance| instance.chain.first())
            .map(|script| script.name.to_ascii_lowercase())
            .collect()
    }

    /// Scripts that will handle this event, including inherited state handlers.
    #[must_use]
    pub fn event_scripts(&self, object: ObjectId, name: &str) -> Vec<String> {
        let name = name.to_ascii_lowercase();
        self.objects
            .get(&object)
            .into_iter()
            .flat_map(|object| &object.instances)
            .filter_map(|id| self.instances.get(id.0))
            .filter_map(|instance| {
                instance
                    .chain
                    .iter()
                    .find_map(|script| {
                        script
                            .functions
                            .get(&(instance.state.to_ascii_lowercase(), name.clone()))
                            .or_else(|| script.functions.get(&(String::new(), name.clone())))
                            .map(|function| (script, function))
                    })
                    .filter(|(_, function)| !function.is_native())
                    .map(|(script, _)| script.name.to_ascii_lowercase())
            })
            .collect()
    }

    pub(super) fn record_event_health(&mut self, id: ThreadId, name: &str) {
        let Some(frame) = self.threads.last().and_then(|thread| thread.frames.first()) else {
            return;
        };
        let key = key(frame);
        let name = name.to_ascii_lowercase();
        self.health
            .rates
            .entry(key.clone())
            .or_default()
            .entry(name.clone())
            .or_default()
            .record(self.menu_time, 1);
        self.health
            .total_rates
            .entry(name.clone())
            .or_default()
            .record(self.menu_time, 1);
        self.health
            .script_rates
            .entry(key.1.clone())
            .or_default()
            .entry(name.clone())
            .or_default()
            .record(self.menu_time, 1);
        let plugin = key
            .0
            .and_then(|object| self.health_owner(object, &key.1))
            .unwrap_or("<unattributed>")
            .to_owned();
        self.health
            .plugin_rates
            .entry((plugin, key.1.clone()))
            .or_default()
            .entry(name.clone())
            .or_default()
            .record(self.menu_time, 1);
        self.health.queued.insert(id, (key, name));
    }

    pub(super) fn record_running_health(
        &mut self,
        object: Option<ObjectId>,
        script: &str,
        ns: u64,
    ) {
        let scripts = self.health.running.entry(object).or_default();
        if let Some(elapsed) = scripts.get_mut(script) {
            *elapsed = elapsed.saturating_add(ns);
        } else {
            scripts.insert(script.into(), ns);
        }
    }

    pub(super) fn advance_health(&mut self, seconds: f64, hours: f64, menu: bool) {
        self.health.frame.clear();
        self.health.running.clear();
        for thread in &self.threads {
            let Some(frame) = thread.frames.last() else {
                continue;
            };
            if !suspended(&thread.state) {
                continue;
            }
            let fraction = match thread.state {
                ThreadState::Sleeping(until) if !menu && seconds > 0.0 => {
                    (until - self.time) / seconds
                }
                ThreadState::SleepingMenu(until) if seconds > 0.0 => {
                    (until - self.menu_time) / seconds
                }
                ThreadState::SleepingGame(until) if !menu && hours > 0.0 => {
                    (until - self.game_hours) / hours
                }
                _ => 1.0,
            }
            .clamp(0.0, 1.0);
            self.health
                .frame
                .entry(key(frame))
                .or_default()
                .waiting_seconds += seconds * fraction;
        }
    }

    pub(super) fn served_health(&mut self, id: ThreadId) {
        self.health.deferred.remove(&id);
        self.health.queued.remove(&id);
    }

    pub(super) fn budget_health(&mut self, exhausted: bool) {
        if !self.health.deferred.is_empty() {
            let runnable: HashSet<_> = self
                .threads
                .iter()
                .filter(|thread| thread.state == ThreadState::Runnable)
                .map(|thread| thread.id)
                .collect();
            self.health.deferred.retain(|id, _| runnable.contains(id));
        }
        if exhausted {
            for thread in &self.threads {
                if thread.state == ThreadState::Runnable {
                    self.health
                        .deferred
                        .entry(thread.id)
                        .or_insert(self.menu_time);
                }
            }
        }
    }

    /// Read live scheduler state and the current frame's durations, without resets.
    /// A suspended stack is counted once in each defining script it contains.
    #[must_use]
    pub fn health_rows(&self) -> Vec<ScriptHealth> {
        let mut rows = self.health.frame.clone();
        for (object, scripts) in &self.health.running {
            for (script, elapsed) in scripts {
                let row = rows
                    .entry((*object, script.to_ascii_lowercase()))
                    .or_default();
                row.running_ns = row.running_ns.saturating_add(*elapsed);
            }
        }
        for (key, rates) in &self.health.rates {
            let row = rows.entry(key.clone()).or_default();
            for (event, rate) in rates {
                row.events.entry(event.clone()).or_default().peak_per_second = rate.peak();
            }
        }
        for (key, event) in self.health.queued.values() {
            rows.entry(key.clone())
                .or_default()
                .events
                .entry(event.clone())
                .or_default()
                .queued += 1;
        }
        for thread in &self.threads {
            if let Some(since) = self.health.deferred.get(&thread.id) {
                if let Some(frame) = thread.frames.last() {
                    let row = rows.entry(key(frame)).or_default();
                    row.backlog += 1;
                    row.oldest_wait_seconds = row.oldest_wait_seconds.max(self.menu_time - since);
                }
            }
            if suspended(&thread.state) {
                let keys: BTreeSet<_> = thread.frames.iter().map(key).collect();
                for key in keys {
                    rows.entry(key).or_default().suspended_stacks += 1;
                }
            }
        }
        rows.into_iter()
            .map(|((object, script), health)| ScriptHealth {
                object,
                script,
                health,
            })
            .collect()
    }

    /// Aggregate stack counts and frame durations. Event peaks are measured across
    /// all deliveries, rather than adding peaks from different script instances.
    #[must_use]
    pub fn health(&self) -> PapyrusHealth {
        let mut health = PapyrusHealth::default();
        for row in self.health_rows() {
            health.add(&row.health);
        }
        health.suspended_stacks = self
            .threads
            .iter()
            .filter(|thread| suspended(&thread.state))
            .count() as u64;
        for (name, rate) in &self.health.total_rates {
            health
                .events
                .entry(name.clone())
                .or_default()
                .peak_per_second = rate.peak();
        }
        health
    }
}
