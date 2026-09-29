// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib actor access backed by the shared runtime world. The API follows
//! the local `Actor.h`, `ActorEquipManager.h` and `ActorState.h` references.

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::ni::NiPoint3;
use soe_commonlib_shim::refr::{
    Actor as ShimActor, ActorApiError, ActorHandle, ActorState, ActorValue, ObjectRef,
};
use soe_esm::GlobalFormId;

use crate::world::World;

/// A borrowed view of one actor in the form-backed world.
pub struct WorldActor<'a> {
    world: &'a mut World,
    id: GlobalFormId,
}

impl<'a> WorldActor<'a> {
    /// Resolve an actor reference by its runtime FormID.
    pub fn resolve(world: &'a mut World, form: FormId) -> Option<Self> {
        let id = global(world, form)?;
        if !world.reference(id)?.is_actor {
            return None;
        }
        world.actor(id)?;
        Some(Self { world, id })
    }

    /// Runtime FormID used by the CommonLib-facing handle.
    pub fn handle(&self) -> ActorHandle {
        ActorHandle(self.world.order().runtime_id(self.id))
    }
}

fn global(world: &World, form: FormId) -> Option<GlobalFormId> {
    if form.0 >> 24 == 0xFF {
        let id = GlobalFormId {
            file: soe_esm::CREATED_FILE,
            object: form.0 & 0x00FF_FFFF,
        };
        return world.exists(id).then_some(id);
    }
    world
        .order()
        .winners()
        .map(|(id, _)| id)
        .find(|id| world.order().runtime_id(*id) == form.0)
}

fn actor_value_name(value: ActorValue) -> Option<&'static str> {
    match value.0 {
        24 => Some("health"),
        25 => Some("magicka"),
        26 => Some("stamina"),
        _ => None,
    }
}

impl ObjectRef for WorldActor<'_> {
    fn handle(&self) -> soe_commonlib_shim::refr::ObjectRefHandle {
        self.handle().into()
    }

    fn form_id(&self) -> FormId {
        FormId(self.world.order().runtime_id(self.id))
    }

    fn base(&self) -> FormId {
        FormId(
            self.world
                .reference_ref(self.id)
                .and_then(|reference| reference.base)
                .map_or(0, |base| self.world.order().runtime_id(base)),
        )
    }

    fn position(&self) -> NiPoint3 {
        let position = self
            .world
            .reference_ref(self.id)
            .map_or([0.0; 3], |reference| reference.position);
        NiPoint3::new(position[0], position[1], position[2])
    }

    fn rotation(&self) -> NiPoint3 {
        let rotation = self
            .world
            .reference_ref(self.id)
            .map_or([0.0; 3], |reference| reference.rotation);
        NiPoint3::new(rotation[0], rotation[1], rotation[2])
    }

    fn parent_cell(&self) -> Option<FormId> {
        self.world
            .reference_ref(self.id)
            .and_then(|reference| reference.cell)
            .map(|cell| FormId(self.world.order().runtime_id(cell)))
    }

    fn is_disabled(&self) -> bool {
        self.world
            .reference_ref(self.id)
            .is_some_and(|reference| !reference.enabled)
    }

    fn is_deleted(&self) -> bool {
        self.world
            .reference_ref(self.id)
            .is_some_and(|reference| reference.deleted)
    }

    fn set_base(&mut self, base: FormId) {
        if let Some(base) = global(self.world, base) {
            if let Some(reference) = self.world.reference(self.id) {
                reference.base = Some(base);
            }
        }
    }
}

impl ShimActor for WorldActor<'_> {
    fn actor_handle(&self) -> ActorHandle {
        self.handle()
    }

    fn state(&self) -> ActorState {
        self.world
            .actor_ref(self.id)
            .map_or_else(ActorState::default, |state| ActorState {
                dead: state.dead,
                unconscious: state.unconscious,
                sneaking: state.sneaking,
                weapon_drawn: state.weapon_drawn,
                blocking: state.blocking,
            })
    }

    fn actor_value(&self, value: ActorValue) -> Result<f32, ActorApiError> {
        let name = actor_value_name(value).ok_or(ActorApiError::UnsupportedOperation)?;
        self.world
            .actor_ref(self.id)
            .map(|state| state.values.get(name).current())
            .ok_or(ActorApiError::InvalidActor)
    }

    fn set_actor_value(&mut self, value: ActorValue, amount: f32) -> Result<(), ActorApiError> {
        let name = actor_value_name(value).ok_or(ActorApiError::UnsupportedOperation)?;
        self.world
            .set_actor_value(self.id, name, amount)
            .map_err(|_| ActorApiError::UnsupportedOperation)
    }

    fn race(&self) -> FormId {
        FormId(
            self.world
                .actor_ref(self.id)
                .and_then(|state| state.race)
                .map_or(0, |race| self.world.order().runtime_id(race)),
        )
    }

    fn is_dead(&self) -> bool {
        self.world
            .actor_ref(self.id)
            .is_some_and(|state| state.dead)
    }

    fn is_player(&self) -> bool {
        self.world.is_player(self.id)
    }

    fn add_spell(&mut self, spell: FormId) -> bool {
        let Some(spell) = global(self.world, spell) else {
            return false;
        };
        self.world
            .actor(self.id)
            .is_some_and(|state| state.spells.insert(spell))
    }

    fn add_perk(&mut self, perk: FormId) -> bool {
        let Some(perk) = global(self.world, perk) else {
            return false;
        };
        self.world
            .actor(self.id)
            .is_some_and(|state| state.perks.insert(perk))
    }

    fn add_item(&mut self, item: FormId, count: u32) -> bool {
        let Some(item) = global(self.world, item) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        self.world
            .add_item(self.id, item, count.min(i32::MAX as u32) as i32);
        true
    }
}

/// The game implementation of the shim's CommonLib equip manager surface.
pub struct WorldActorEquipManager<'a> {
    world: &'a mut World,
}

impl<'a> WorldActorEquipManager<'a> {
    pub fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    fn actor_id(&self, handle: ActorHandle) -> Option<GlobalFormId> {
        global(self.world, FormId(handle.0))
    }
}

impl soe_commonlib_shim::refr::ActorEquipManager for WorldActorEquipManager<'_> {
    fn equip_object(&mut self, actor: ActorHandle, object: FormId) -> Result<(), ActorApiError> {
        let actor = self.actor_id(actor).ok_or(ActorApiError::InvalidActor)?;
        let object = global(self.world, object).ok_or(ActorApiError::UnknownForm)?;
        self.world
            .equip_item(actor, object)
            .map_err(|_| ActorApiError::UnsupportedOperation)
    }

    fn unequip_object(&mut self, actor: ActorHandle, object: FormId) -> Result<(), ActorApiError> {
        let actor = self.actor_id(actor).ok_or(ActorApiError::InvalidActor)?;
        let object = global(self.world, object).ok_or(ActorApiError::UnknownForm)?;
        self.world
            .unequip_item(actor, object)
            .map_err(|_| ActorApiError::UnsupportedOperation)
    }
}
