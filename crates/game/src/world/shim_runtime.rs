// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib shim reads backed directly by the runtime world.

use soe_commonlib_shim::world_runtime::{FormKey, WorldState};

use super::World;
use soe_esm::GlobalFormId;

fn key(id: GlobalFormId) -> FormKey {
    FormKey {
        file: id.file,
        object: id.object,
    }
}

fn id(key: FormKey) -> GlobalFormId {
    GlobalFormId {
        file: key.file,
        object: key.object,
    }
}

impl WorldState for World {
    fn player(&mut self) -> Option<FormKey> {
        Some(key(World::player(self)))
    }

    fn loaded_actors(&self) -> Vec<FormKey> {
        World::loaded_actors(self).into_iter().map(key).collect()
    }

    fn reference_position(&mut self, form: FormKey) -> Option<[f32; 3]> {
        self.reference(id(form)).map(|reference| reference.position)
    }

    fn actor_value(&mut self, form: FormKey, name: &str) -> Option<f32> {
        self.actor(id(form))?;
        Some(World::actor_value(self, id(form), name))
    }

    fn actor_is_dead(&mut self, form: FormKey) -> Option<bool> {
        Some(self.actor(id(form))?.dead)
    }
}
