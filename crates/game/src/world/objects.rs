// SPDX-License-Identifier: GPL-3.0-or-later

//! VM objects: the two-way table between a `Handle` (a form or a quest
//! alias) and the Papyrus object that stands for it, created on first use.

use soe_esm::GlobalFormId;
use soe_papyrus::{ObjectId, Value, Vm};

use super::{Alloc, Handle, Pending, World};

impl World {
    /// Retire expired effects after their callbacks and script references drain.
    pub(crate) fn collect_effect_objects(&mut self, vm: &mut Vm) {
        let candidates: Vec<_> = self
            .handles
            .iter()
            .filter_map(|(object, handle)| match handle {
                Handle::ActiveEffect(id) if !self.active_effects.contains_key(id) => Some(*object),
                _ => None,
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let mut roots: Vec<_> = self.settings.values().cloned().collect();
        for pending in &self.pending {
            match pending {
                Pending::Event { object, args, .. }
                | Pending::Call { object, args, .. }
                | Pending::CallScript { object, args, .. } => {
                    roots.push(Value::Object(*object));
                    roots.extend(args.iter().cloned());
                }
                Pending::BindEffect { object, .. } => roots.push(Value::Object(*object)),
                Pending::ActorDeath { recipients, .. } => roots.extend(
                    recipients
                        .iter()
                        .filter_map(|handle| self.objects.get(handle).copied())
                        .map(Value::Object),
                ),
                Pending::DeferredDeath { .. } => {}
            }
        }
        for object in vm.collect_objects(&candidates, &roots) {
            if let Some(handle) = self.handles.remove(&object) {
                self.objects.remove(&handle);
            }
            self.timers.retain(|timer| timer.object != object);
            self.sleep_listeners.retain(|listener| *listener != object);
            self.stats_listeners.retain(|listener| *listener != object);
            self.animation_listeners
                .retain(|listener| listener.listener != object);
            self.los_listeners
                .retain(|listener| listener.listener != object);
            self.inventory_filters.remove(&object);
        }
    }

    pub fn handle_of(&self, object: ObjectId) -> Option<Handle> {
        self.handles.get(&object).copied()
    }

    pub fn form_of(&self, object: ObjectId) -> Option<GlobalFormId> {
        match self.handle_of(object)? {
            Handle::Form(id) => Some(id),
            Handle::Alias { .. } | Handle::ActiveEffect(_) => None,
        }
    }

    pub fn object_id(&self, handle: Handle) -> Option<ObjectId> {
        self.objects.get(&handle).copied()
    }

    /// The VM object for a handle, created on first use.
    pub fn object_of(&mut self, handle: Handle, alloc: Alloc<'_>) -> ObjectId {
        if let Some(id) = self.objects.get(&handle) {
            return *id;
        }
        if let Handle::Alias { quest, .. } = handle {
            // The alias kind decides the class, so the definition is needed.
            self.quest_state(quest);
        }
        let name = match handle {
            Handle::ActiveEffect(id) => format!("active effect {}", id.0),
            Handle::Form(id) => self
                .editor_id(id)
                .unwrap_or_else(|| format!("{:08X}", self.runtime_id(id))),
            Handle::Alias { quest, id } => {
                let alias = self
                    .quest_state(quest)
                    .and_then(|q| q.def.alias(id).map(|a| a.name.clone()));
                alias.unwrap_or_else(|| format!("alias {id}"))
            }
        };
        let class = self.class_of(handle);
        let object = alloc(&name, class);
        self.objects.insert(handle, object);
        self.handles.insert(object, handle);
        object
    }

    pub fn form_value(&mut self, id: Option<GlobalFormId>, alloc: Alloc<'_>) -> Value {
        match id {
            Some(id) if self.exists(id) => Value::Object(self.object_of(Handle::Form(id), alloc)),
            _ => Value::None,
        }
    }

    pub fn form_arg(&self, v: &Value) -> Option<GlobalFormId> {
        v.as_object().and_then(|o| self.form_of(o))
    }
}
