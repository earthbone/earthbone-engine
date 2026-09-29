// SPDX-License-Identifier: MIT

//! References and actors: the runtime objects the round-2 `game` crate owns.
//! The shim names them as handles, since a plugin holds a `RE::ObjectRefHandle`
//! or `RE::ActorHandle` across frames and the engine decides what backs it.

use crate::form::FormId;
use crate::ni::NiPoint3;

/// Runtime data exposed by CommonLib's ExtraLeveledItemBase.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExtraLeveledItemBase {
    pub leveled_item: FormId,
}

/// Runtime light override values carried by ExtraLightData.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ExtraLightData {
    pub fov: f32,
    pub fade: f32,
    pub end_distance_cap: f32,
    pub shadow_depth_bias: f32,
    pub unknown: u8,
}

/// A keyword and reference pair from ExtraLinkedRefChildren.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LinkedRefChild {
    pub keyword: FormId,
    pub reference: ObjectRefHandle,
}

/// Map marker data exposed by ExtraMapMarker.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtraMapMarker {
    pub name: String,
    pub flags: u8,
    pub marker_type: u16,
}

/// A keyword and unresolved target id from ExtraMissingLinkedRefIDs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MissingLinkedRefId {
    pub keyword: FormId,
    pub linked_reference: FormId,
}

/// Read-only view of the five reference extra-data types in this batch.
/// Missing values mean the reference has no corresponding extra data.
pub trait ReferenceExtraData {
    fn leveled_item_base(&self) -> Option<ExtraLeveledItemBase>;
    fn light_data(&self) -> Option<ExtraLightData>;
    fn linked_ref_children(&self) -> &[LinkedRefChild];
    fn map_marker(&self) -> Option<ExtraMapMarker>;
    fn missing_linked_ref_ids(&self) -> &[MissingLinkedRefId];
}

/// Executable-address and vtable hooks have no implementation in this engine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UnsupportedHook {
    pub name: &'static str,
    pub reason: &'static str,
}

pub const EXTRA_DATA_HOOKS: &[UnsupportedHook] = &[UnsupportedHook {
    name: "CommonLib extra-data vtable hooks",
    reason: "Skyrim executable vtables have no target in the Rust engine",
}];

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
    /// The `SetBaseObject` path Base Object Swapper needs at load time.
    fn set_base(&mut self, base: FormId);
}

/// The `RE::Actor` surface: the object-ref surface plus actor state.
pub trait Actor: ObjectRef {
    fn actor_handle(&self) -> ActorHandle;
    fn actor_value(&self, av: ActorValue) -> f32;
    fn set_actor_value(&mut self, av: ActorValue, value: f32);
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
