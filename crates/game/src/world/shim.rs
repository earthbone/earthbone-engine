// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib-facing views over existing world state. These adapters do not
//! create a second store or expose Skyrim executable hooks.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    ActivateRefChildren, AliasInstance, AliasInstances, Calendar as CalendarApi,
    Character as CharacterApi, DefaultObjects as DefaultObjectsApi, UnsupportedHook,
};
use soe_esm::GlobalFormId;

use super::World;

fn form_id(world: &World, id: GlobalFormId) -> FormId {
    FormId(world.runtime_id(id))
}

fn global_id(world: &World, id: FormId) -> Option<GlobalFormId> {
    world.from_runtime_id(id.0)
}

pub struct CalendarView<'a>(&'a World);

impl CalendarApi for CalendarView<'_> {
    fn year(&self) -> u32 {
        self.0.calendar().year
    }

    fn month(&self) -> u32 {
        self.0.calendar().month
    }

    fn day(&self) -> u32 {
        self.0.calendar().day
    }

    fn hour(&self) -> f64 {
        self.0.calendar().hour
    }

    fn days_passed(&self) -> f64 {
        self.0.clock.game_days
    }

    fn timescale(&self) -> f64 {
        self.0.clock.timescale
    }
}

pub struct CharacterView<'a> {
    world: &'a World,
    id: GlobalFormId,
}

impl CharacterApi for CharacterView<'_> {
    fn form_id(&self) -> FormId {
        form_id(self.world, self.id)
    }

    fn actor_value(&self, name: &str) -> Option<f32> {
        self.world
            .actor_ref(self.id)
            .map(|actor| actor.values.get(name).current())
    }

    fn is_dead(&self) -> bool {
        self.world
            .actor_ref(self.id)
            .is_some_and(|actor| actor.dead)
    }

    fn race(&self) -> Option<FormId> {
        self.world
            .actor_ref(self.id)?
            .race
            .map(|race| form_id(self.world, race))
    }
}

pub struct DefaultObjectsView<'a>(&'a World);

impl DefaultObjectsApi for DefaultObjectsView<'_> {
    fn get(&self, key: [u8; 4]) -> Result<Option<FormId>, String> {
        crate::footsteps::Footsteps::new(&self.0.order)
            .default_object(key)
            .map(|id| id.map(|id| form_id(self.0, id)))
            .map_err(|error| error.to_string())
    }
}

impl AliasInstances for World {
    fn instances(&self, reference: FormId) -> Vec<AliasInstance> {
        let Some(reference) = global_id(self, reference) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for (quest_id, state) in &self.quests {
            for alias in &state.def.aliases {
                if state.aliases.get(&alias.id).copied().flatten() != Some(reference) {
                    continue;
                }
                let packages = alias
                    .packages
                    .iter()
                    .filter_map(|raw| self.resolve_from(*quest_id, *raw))
                    .map(|id| form_id(self, id))
                    .collect();
                entries.push(AliasInstance {
                    quest: form_id(self, *quest_id),
                    alias_id: alias.id,
                    reference: Some(form_id(self, reference)),
                    packages,
                });
            }
        }
        entries.sort_by_key(|entry| (entry.quest.0, entry.alias_id));
        entries
    }
}

impl ActivateRefChildren for World {
    fn children(&self, _reference: FormId) -> Result<Vec<FormId>, UnsupportedHook> {
        Err(UnsupportedHook::ActivateReferenceChildren)
    }

    fn timer(&self, _reference: FormId) -> Result<f32, UnsupportedHook> {
        Err(UnsupportedHook::ActivateReferenceChildren)
    }

    fn activate_children(&self, _reference: FormId) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook::ActivateReferenceChildren)
    }
}

impl World {
    pub fn shim_calendar(&self) -> CalendarView<'_> {
        CalendarView(self)
    }

    pub fn shim_character(&self, id: GlobalFormId) -> Option<CharacterView<'_>> {
        self.reference_ref(id)
            .filter(|reference| reference.is_actor)?;
        Some(CharacterView { world: self, id })
    }

    pub fn shim_default_objects(&self) -> DefaultObjectsView<'_> {
        DefaultObjectsView(self)
    }
}
