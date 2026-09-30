// SPDX-License-Identifier: MIT

//! Reference and actor contracts adapted from the CommonLibSSE-NG reference
//! headers. The game crate implements them with handles into its shared world.

use crate::form::FormId;
use crate::ni::NiPoint3;

/// `RE::ObjectRefHandle`: a reference that may have been unloaded since.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjectRefHandle(pub u32);

/// `RE::ActorHandle`, the same handle space narrowed to actors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorHandle(pub u32);

/// `RE::ActorPtr`, represented as an optional stable handle rather than a
/// pointer into engine-owned memory.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ActorPtr(pub Option<ActorHandle>);

/// A read-only snapshot of the `RE::ActorState` fields the runtime models.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ActorState {
    pub dead: bool,
    pub unconscious: bool,
    pub sneaking: bool,
    pub weapon_drawn: bool,
    pub blocking: bool,
}

/// Unsupported binary hooks are returned as data so source ports can fail
/// explicitly instead of silently installing a no-op.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookError {
    AddressPatch,
    CallDetour,
    VtableOverride,
    Trampoline,
}

/// A rejected operation at the actor compatibility boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActorApiError {
    InvalidActor,
    UnknownForm,
    UnsupportedOperation,
}

/// The live engine operations exposed by `RE::ActorEquipManager`.
pub trait ActorEquipManager {
    fn equip_object(&mut self, actor: ActorHandle, object: FormId) -> Result<(), ActorApiError>;
    fn unequip_object(&mut self, actor: ActorHandle, object: FormId) -> Result<(), ActorApiError>;
    fn reject_hook(&self, hook: HookError) -> Result<(), HookError> {
        Err(hook)
    }
}

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
    /// The `SetBaseObject` path Base Object Swapper needs at load time.
    fn set_base(&mut self, base: FormId);
}

/// The `RE::Actor` surface: the object-ref surface plus actor state.
pub trait Actor: ObjectRef {
    fn actor_handle(&self) -> ActorHandle;
    fn actor_ptr(&self) -> ActorPtr {
        ActorPtr(Some(self.actor_handle()))
    }
    fn state(&self) -> ActorState;
    fn actor_value(&self, av: ActorValue) -> Result<f32, ActorApiError>;
    fn set_actor_value(&mut self, av: ActorValue, value: f32) -> Result<(), ActorApiError>;
    fn race(&self) -> FormId;
    fn is_dead(&self) -> bool;
    fn is_player(&self) -> bool;
    /// Spells, perks and items the distributors add: one entry point each,
    /// returning whether anything changed.
    fn add_spell(&mut self, spell: FormId) -> bool;
    fn add_perk(&mut self, perk: FormId) -> bool;
    fn add_item(&mut self, item: FormId, count: u32) -> bool;
}

/// `RE::ActorValue`, the numeric id the engine's `ActorValueInfo` table uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorValue(pub u32);

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
