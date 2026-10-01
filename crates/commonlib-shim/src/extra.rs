// SPDX-License-Identifier: MIT

//! Read-only views of the extra data attached to a live item stack.

use crate::form::FormId;

/// CommonLib extra-data tags exposed by this batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExtraDataType {
    LeveledCreature = 0x2D,
    Hotkey = 0x49,
    Enchantment = 0x9B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtraEnchantment {
    pub enchantment: FormId,
    pub charge: u16,
    /// Stack save data does not carry CommonLib's runtime-only flag.
    pub remove_on_unequip: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtraHotkey {
    /// `None` is the CommonLib unbound value, `Some(0..=7)` is a hotkey slot.
    pub slot: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtraLeveledCreature {
    pub original_base: FormId,
    pub template_base: FormId,
}

/// Backing state supplied by the engine. Implementations read the current stack.
pub trait ExtraDataSource {
    fn has_type(&self, kind: ExtraDataType) -> bool;
    fn enchantment(&self) -> Option<ExtraEnchantment>;
    fn hotkey(&self) -> Option<ExtraHotkey>;
    fn leveled_creature(&self) -> Option<ExtraLeveledCreature>;
}

/// Read-only CommonLib view over an engine-owned extra-data list.
pub struct ExtraDataList<'a> {
    source: &'a dyn ExtraDataSource,
}

impl<'a> ExtraDataList<'a> {
    pub fn new(source: &'a dyn ExtraDataSource) -> Self {
        Self { source }
    }

    pub fn has_type(&self, kind: ExtraDataType) -> bool {
        self.source.has_type(kind)
    }

    pub fn enchantment(&self) -> Option<ExtraEnchantment> {
        self.source.enchantment()
    }

    pub fn hotkey(&self) -> Option<ExtraHotkey> {
        self.source.hotkey()
    }

    pub fn leveled_creature(&self) -> Option<ExtraLeveledCreature> {
        self.source.leveled_creature()
    }
}

/// These engine-private mutation hooks have no implementation in the shim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedExtraHook {
    Add,
    Remove,
    RemoveByType,
    SetEnchantment,
}

impl UnsupportedExtraHook {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Add => "ExtraDataList::Add mutates engine-owned list state",
            Self::Remove => "ExtraDataList::Remove mutates engine-owned list state",
            Self::RemoveByType => "ExtraDataList::RemoveByType mutates engine-owned list state",
            Self::SetEnchantment => "ExtraDataList::SetEnchantment mutates engine-owned list state",
        }
    }
}
