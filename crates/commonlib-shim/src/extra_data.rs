// SPDX-License-Identifier: MIT

//! Reference extra-data views for fields represented by the runtime model.
//!
//! The value shapes follow CommonLibSSE-NG's `ExtraMissingRefIDs`,
//! `ExtraNorthRotation`, `ExtraOutfitItem`, `ExtraPoison` and `ExtraTeleport`
//! declarations. They are data views and contain no Skyrim memory layout.

use crate::form::FormId;
use crate::ni::NiPoint3;
use crate::refr::ObjectRefHandle;

/// A missing activate-parent link and its authored delay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivateParentId {
    pub reference: FormId,
    pub activate_delay: f32,
}

/// Door destination data carried by `ExtraTeleport`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoorTeleportData {
    pub linked_door: ObjectRefHandle,
    pub position: NiPoint3,
    pub rotation: NiPoint3,
    pub flags: i8,
}

/// The five extra-data values bound by SHM-10.
#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceExtra {
    MissingRefIds(Vec<ActivateParentId>),
    NorthRotation(f32),
    OutfitItem(FormId),
    Poison { poison: FormId, count: u32 },
    Teleport(DoorTeleportData),
}

/// Extra-data kinds understood by the shim boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceExtraKind {
    MissingRefIds,
    NorthRotation,
    OutfitItem,
    Poison,
    Teleport,
}

/// Binary patch sites cannot target this runtime's Rust implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHook {
    pub name: &'static str,
}

impl std::fmt::Display for UnsupportedHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported binary hook: {}", self.name)
    }
}

impl std::error::Error for UnsupportedHook {}

/// Runtime-backed access to reference extra data.
pub trait ReferenceExtraData {
    /// Read a value from a live reference. `None` means the value is absent;
    /// unsupported kinds are reported separately by `supports_extra`.
    fn extra(&self, reference: ObjectRefHandle, kind: ReferenceExtraKind)
        -> Option<ReferenceExtra>;

    /// Whether the backing runtime has an authoritative representation.
    fn supports_extra(&self, kind: ReferenceExtraKind) -> bool;
}

/// Reject a binary hook request with a diagnostic value.
///
/// # Errors
/// Always returns `UnsupportedHook`, because executable patching has no target
/// in the Rust runtime.
pub fn reject_hook(name: &'static str) -> Result<(), UnsupportedHook> {
    Err(UnsupportedHook { name })
}
