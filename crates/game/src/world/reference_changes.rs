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
        if !self.watched.insert(id) {
            return;
        }
        let _ = world.reference_enabled(id);
        let mut state = world.reference_changes.borrow_mut();
        let observers = state.observers.entry(id).or_default();
        observers.retain(|observer| observer.strong_count() != 0);
        observers.push(Arc::downgrade(&self.queue));
        drop(state);
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id);
    }

    /// Return only attached references whose effective presentation changed.
    pub fn changed(&mut self, world: &World) -> Vec<GlobalFormId> {
        let dirty = std::mem::take(
            &mut *self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        dirty
            .into_iter()
            .filter(|id| {
                let current = (
                    world.ref_placement(*id),
                    world.destruction_model(*id),
                    world.reference_ref(*id).is_some_and(|state| state.opened),
                );
                if self.applied.get(id) == Some(&current) {
                    false
                } else {
                    self.applied.insert(*id, current);
                    true
                }
            })
            .collect()
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
