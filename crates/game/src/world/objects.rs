// SPDX-License-Identifier: GPL-3.0-or-later

//! VM objects: the two-way table between a `Handle` (a form or a quest
//! alias) and the Papyrus object that stands for it, created on first use.

use soe_esm::GlobalFormId;
use soe_papyrus::{ObjectId, Value};

use super::{Alloc, Handle, World};

impl World {
    pub fn handle_of(&self, object: ObjectId) -> Option<Handle> {
        self.handles.get(&object).copied()
    }

    pub fn form_of(&self, object: ObjectId) -> Option<GlobalFormId> {
        match self.handle_of(object)? {
            Handle::Form(id) => Some(id),
            Handle::Alias { .. } => None,
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
