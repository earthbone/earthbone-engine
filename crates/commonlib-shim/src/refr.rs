// SPDX-License-Identifier: MIT

//! References and actors: the runtime objects the round-2 `game` crate owns.
//! The shim names them as handles, since a plugin holds a `RE::ObjectRefHandle`
//! or `RE::ActorHandle` across frames and the engine decides what backs it.

use crate::form::FormId;
use crate::ni::NiPoint3;

/// `RE::ObjectRefHandle`: a reference that may have been unloaded since.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjectRefHandle(pub u32);

/// `RE::ActorHandle`, the same handle space narrowed to actors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorHandle(pub u32);

impl From<ActorHandle> for ObjectRefHandle {
    fn from(h: ActorHandle) -> Self {
        Self(h.0)
    }
}

/// The `RE::TESObjectREFR` surface the sample plugins read and write.
pub trait ObjectRef {
    fn handle(&self) -> ObjectRefHandle;
    fn form_id(&self) -> FormId;
    fn base(&self) -> FormId;
    fn position(&self) -> NiPoint3;
    fn rotation(&self) -> NiPoint3;
    fn parent_cell(&self) -> Option<FormId>;
    fn is_disabled(&self) -> bool;
    fn is_deleted(&self) -> bool;
    /// `SetBaseObject` has no runtime implementation for this reference yet.
    fn set_base(&mut self, base: FormId) -> Result<(), Unsupported>;
}

/// The `RE::Actor` surface: the object-ref surface plus actor state.
pub trait Actor: ObjectRef {
    fn actor_handle(&self) -> ActorHandle;
    fn actor_value(&self, av: ActorValue) -> f32;
    fn set_actor_value(&mut self, av: ActorValue, value: f32) -> Result<(), Unsupported>;
    fn race(&self) -> FormId;
    fn is_dead(&self) -> bool;
    fn is_player(&self) -> bool;
    fn weapon_state(&self) -> WeaponState;
    /// Spells, perks and items the distributors add: one entry point each,
    /// returning whether anything changed.
    fn add_spell(&mut self, spell: FormId) -> bool;
    fn add_perk(&mut self, perk: FormId) -> bool;
    fn add_item(&mut self, item: FormId, count: u32) -> bool;
}

/// `RE::ActorValue`, the numeric id the engine's `ActorValueInfo` table uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorValue(pub u32);

/// `RE::WEAPON_STATE`, retaining each runtime transition state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum WeaponState {
    Sheathed = 0,
    WantToDraw = 1,
    Drawing = 2,
    Drawn = 3,
    WantToSheathe = 4,
    Sheathing = 5,
}

/// Nullable reference identity corresponding to `RE::TESObjectREFRPtr`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ObjectRefPtr(pub Option<ObjectRefHandle>);

impl ObjectRefPtr {
    pub fn is_null(self) -> bool {
        self.0.is_none()
    }
}

/// An operation the runtime cannot safely implement through this API yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Unsupported {
    pub operation: &'static str,
}

impl ActorValue {
    pub const HEALTH: Self = Self(24);
    pub const MAGICKA: Self = Self(25);
    pub const STAMINA: Self = Self(26);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_actor_handle_is_an_object_handle() {
        let a = ActorHandle(7);
        let o: ObjectRefHandle = a.into();
        assert_eq!(o, ObjectRefHandle(7));
    }
}
