// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient reference presentation invalidation for PRF-34. Enable dependency
//! edges follow UESP REFR XESP and xEdit wbXESP. World remains authoritative.

use super::{RefPlacement, World};
use soe_esm::GlobalFormId;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, Weak};

type Queue = Mutex<BTreeSet<GlobalFormId>>;
type Presentation = (Option<RefPlacement>, Option<String>, bool);

/// Independent host observer. Attaching references queues their initial state.
#[derive(Default)]
pub struct ReferenceChanges {
    queue: Arc<Queue>,
    applied: HashMap<GlobalFormId, Presentation>,
    watched: BTreeSet<GlobalFormId>,
}

impl ReferenceChanges {
    /// Subscribe once to a resident reference, including its enable parents.
    pub fn watch(&mut self, world: &World, id: GlobalFormId) {
        if self.watch_quiet(world, id).is_some() {
            self.queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(id);
        }
    }

    /// Subscribe like [`Self::watch`] without queueing the initial state, for a
    /// host that already built that state. Returns the effective enable state
    /// when this call subscribed, and `None` when the reference was already
    /// watched. A later change queues the reference as usual.
    pub fn watch_quiet(&mut self, world: &World, id: GlobalFormId) -> Option<bool> {
        if !self.watched.insert(id) {
            return None;
        }
        let enabled = world.reference_enabled(id);
        let mut state = world.reference_changes.borrow_mut();
        let observers = state.observers.entry(id).or_default();
        observers.retain(|observer| observer.strong_count() != 0);
        let mine = Arc::downgrade(&self.queue);
        if !observers.iter().any(|observer| observer.ptr_eq(&mine)) {
            observers.push(mine);
        }
        Some(enabled)
    }

    /// Whether the reference is subscribed.
    #[must_use]
    pub fn is_watched(&self, id: GlobalFormId) -> bool {
        self.watched.contains(&id)
    }

    /// Stop reporting a reference that left residency. A later `watch` queues
    /// its state again.
    pub fn unwatch(&mut self, id: GlobalFormId) {
        if self.watched.remove(&id) {
            self.applied.remove(&id);
            self.queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
        }
    }

    /// Return only attached references whose effective presentation changed.
    pub fn changed(&mut self, world: &World) -> Vec<GlobalFormId> {
        self.changed_until(world, None)
    }

    /// Like [`Self::changed`], but stops reading the queue once `budget` has
    /// passed, after at least one reference. The rest stay queued for the next
    /// call, so a burst of changes spreads over several frames.
    pub fn changed_within(
        &mut self,
        world: &World,
        budget: std::time::Duration,
    ) -> Vec<GlobalFormId> {
        self.changed_until(world, Some((std::time::Instant::now(), budget)))
    }

    fn changed_until(
        &mut self,
        world: &World,
        limit: Option<(std::time::Instant, std::time::Duration)>,
    ) -> Vec<GlobalFormId> {
        let mut changed = Vec::new();
        let mut first = true;
        loop {
            if !first && limit.is_some_and(|(start, budget)| start.elapsed() >= budget) {
                break;
            }
            first = false;
            let Some(id) = self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_first()
            else {
                break;
            };
            if !self.watched.contains(&id) {
                continue;
            }
            let current = (
                world.ref_placement(id),
                world.destruction_model(id),
                world.reference_ref(id).is_some_and(|state| state.opened),
            );
            if self.applied.get(&id) != Some(&current) {
                self.applied.insert(id, current);
                changed.push(id);
            }
        }
        changed
    }
}

#[derive(Default)]
pub(super) struct State {
    pub enabled: HashMap<GlobalFormId, bool>,
    pub children: HashMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    observers: HashMap<GlobalFormId, Vec<Weak<Queue>>>,
}

impl World {
    pub(super) fn invalidate_reference(&self, id: GlobalFormId) {
        self.contact_references.borrow_mut().invalidate(id);
        self.light_references.borrow_mut().invalidate(id);
        let mut changes = self.reference_changes.borrow_mut();
        let mut pending = vec![id];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            changes.enabled.remove(&id);
            if let Some(observers) = changes.observers.get_mut(&id) {
                observers.retain(|observer| {
                    if let Some(queue) = observer.upgrade() {
                        queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .insert(id);
                        true
                    } else {
                        false
                    }
                });
            }
            if let Some(children) = changes.children.get(&id) {
                pending.extend(children.iter().copied());
            }
        }
    }

    pub(super) fn invalidate_all_references(&self) {
        let ids: Vec<_> = self
            .reference_changes
            .borrow()
            .enabled
            .keys()
            .copied()
            .collect();
        for id in ids {
            self.invalidate_reference(id);
        }
    }
}
