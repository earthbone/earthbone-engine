// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved FormIDs remapped through plugin names into the live form table.
//! RefID and regular/light slot rules follow UESP Save File Format and
//! FallrimTools RefID.java and PluginInfo.java. This module applies the global
//! variable table. Other world state and Papyrus bindings remain separate.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{GlobalFormId, LoadOrder, Slot, Tag};
use soe_save::{GlobalData, RefId, Save, WString};

use crate::{world::DYNAMIC_FILE, World};

/// A failure to identify or apply saved state without changing the world.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RestoreError {
    #[error("cannot restore actor templates: {0}")]
    ActorTemplates(String),
    #[error("cannot restore inventory: {0}")]
    Inventory(String),
    #[error(transparent)]
    Save(#[from] soe_save::Error),
    #[error("invalid saved plugin lists: {0}")]
    PluginLists(String),
    #[error("saved plugin {0} is not loaded")]
    MissingPlugin(String),
    #[error("plugin {0} changed between regular and light format")]
    PluginKind(String),
    #[error("saved reference {0:#08x} has no valid FormID")]
    Reference(u32),
    #[error("saved FormID {0:#010x} names a plugin slot outside the saved lists")]
    PluginSlot(u32),
    #[error("expected one saved global-variable block, found {0}")]
    GlobalBlocks(usize),
    #[error("saved global {0:#08x} does not name a live GLOB record")]
    GlobalTarget(u32),
    #[error("saved global {0} occurs more than once")]
    DuplicateGlobal(GlobalFormId),
    #[error("cannot restore calendar: {0}")]
    Calendar(String),
    #[error("cannot restore quest {id}: {reason}")]
    Quest { id: GlobalFormId, reason: String },
    #[error("cannot restore NPC {id}: {reason}")]
    Npc { id: GlobalFormId, reason: String },
    #[error("cannot restore list {id}: {reason}")]
    List { id: GlobalFormId, reason: String },
    #[error("cannot restore BOOK, NOTE or KEYM {id}: {reason}")]
    BookNoteKey { id: GlobalFormId, reason: String },
    #[error("cannot restore reference {id}: {reason}")]
    WorldReference { id: GlobalFormId, reason: String },
}

/// Plugin positions in a save mapped to positions in the current load order.
/// This resolves identity only. A created form still needs its change form
/// applied, and a plugin form can be missing or deleted in the current files.
pub struct SavedFormMap<'a> {
    save: &'a Save,
    regular: Vec<u16>,
    light: Vec<u16>,
}

impl<'a> SavedFormMap<'a> {
    /// Match saved plugin names without regard to ASCII case.
    ///
    /// # Errors
    /// Missing or ambiguous plugins, incompatible regular/light flags, invalid
    /// list sizes.
    pub fn new(save: &'a Save, order: &LoadOrder) -> Result<Self, RestoreError> {
        if save.plugins.is_empty() || save.plugins.len() > 254 || save.light_plugins.len() > 4096 {
            return Err(RestoreError::PluginLists("invalid slot counts".into()));
        }
        let mut current = BTreeMap::new();
        for (index, file) in order.files().iter().enumerate() {
            let name = file.name.to_ascii_lowercase();
            if current.insert(name, (index, file.slot)).is_some() {
                return Err(RestoreError::PluginLists(format!(
                    "duplicate current plugin {}",
                    file.name
                )));
            }
        }
        let mut seen = BTreeSet::new();
        let mut remap = |plugins: &[WString], expected: Slot| {
            plugins
                .iter()
                .map(|name| {
                    let name = name.to_text();
                    let key = name.to_ascii_lowercase();
                    if !seen.insert(key.clone()) {
                        return Err(RestoreError::PluginLists(format!(
                            "duplicate saved plugin {name}"
                        )));
                    }
                    let &(index, slot) = current
                        .get(&key)
                        .ok_or_else(|| RestoreError::MissingPlugin(name.clone()))?;
                    if !matches!(
                        (slot, expected),
                        (Slot::Regular(_), Slot::Regular(_)) | (Slot::Light(_), Slot::Light(_))
                    ) {
                        return Err(RestoreError::PluginKind(name));
                    }
                    let index = u16::try_from(index).map_err(|_| {
                        RestoreError::PluginLists(
                            "current load order exceeds identity space".into(),
                        )
                    })?;
                    Ok(index)
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let regular = remap(&save.plugins, Slot::Regular(0))?;
        let light = remap(&save.light_plugins, Slot::Light(0))?;
        Ok(Self {
            save,
            regular,
            light,
        })
    }

    /// Resolve a packed RefID using the save's own FormID array.
    ///
    /// # Errors
    /// An invalid RefID tag, array index or saved plugin slot.
    pub fn reference(&self, reference: RefId) -> Result<Option<GlobalFormId>, RestoreError> {
        let raw = self
            .save
            .resolve(reference)
            .ok_or(RestoreError::Reference(reference.raw()))?;
        self.form_id(raw)
    }

    /// Resolve a runtime FormID from the save, preserving created identities.
    /// Zero is null. No current runtime-slot lookup is used.
    ///
    /// # Errors
    /// A regular or light slot outside the corresponding saved plugin list.
    pub fn form_id(&self, raw: u32) -> Result<Option<GlobalFormId>, RestoreError> {
        if raw == 0 {
            return Ok(None);
        }
        let (file, object) = match raw >> 24 {
            0xFF => (DYNAMIC_FILE, raw & 0x00FF_FFFF),
            0xFE => (
                *self
                    .light
                    .get(((raw >> 12) & 0xFFF) as usize)
                    .ok_or(RestoreError::PluginSlot(raw))?,
                raw & 0xFFF,
            ),
            slot => (
                *self
                    .regular
                    .get(slot as usize)
                    .ok_or(RestoreError::PluginSlot(raw))?,
                raw & 0x00FF_FFFF,
            ),
        };
        Ok(Some(GlobalFormId { file, object }))
    }
}

impl World {
    /// Restore the ESS global-variable table into the live form-backed world.
    /// Validate every entry before applying any of them. Unlisted globals
    /// retain their current values. This neither resumes a save nor restores
    /// scheduler deadlines, quests, objects or script instances. Calendar globals
    /// also initialize the game clock before any execution can observe it.
    ///
    /// # Errors
    /// Invalid plugin mapping, absent or repeated global tables, malformed
    /// values, missing/deleted/non-global targets or repeated entries.
    pub fn restore_vanilla_globals(&mut self, save: &Save) -> Result<usize, RestoreError> {
        let mapping = SavedFormMap::new(save, self.order())?;
        let blocks = save
            .table_1
            .iter()
            .filter(|block| block.kind == GlobalData::GLOBAL_VARIABLES)
            .count();
        if blocks != 1 {
            return Err(RestoreError::GlobalBlocks(blocks));
        }
        let mut values = BTreeMap::new();
        for global in save.globals()?.variables {
            let target = mapping
                .reference(global.form)?
                .ok_or(RestoreError::GlobalTarget(global.form.raw()))?;
            if target.file == DYNAMIC_FILE
                || !self
                    .order()
                    .winner(target)
                    .is_some_and(|record| record.tag == Tag(*b"GLOB") && !record.is_deleted())
            {
                return Err(RestoreError::GlobalTarget(global.form.raw()));
            }
            if values.insert(target, global.value).is_some() {
                return Err(RestoreError::DuplicateGlobal(target));
            }
        }
        self.validate_saved_calendar(&values)?;
        let count = values.len();
        for (id, value) in values {
            self.set_global(id, value);
        }
        Ok(count)
    }

    fn validate_saved_calendar(
        &self,
        values: &BTreeMap<GlobalFormId, f32>,
    ) -> Result<(), RestoreError> {
        let fields = crate::clock::CALENDAR_GLOBALS
            .map(|name| self.form(name).and_then(|id| values.get(&id).copied()));
        if fields.iter().all(Option::is_none) {
            return Ok(());
        }
        let mut calendar = [0.0; 6];
        for (index, field) in fields.into_iter().enumerate() {
            calendar[index] = field.ok_or_else(|| {
                RestoreError::Calendar(format!(
                    "missing saved {}",
                    crate::clock::CALENDAR_GLOBALS[index]
                ))
            })?;
            if !calendar[index].is_finite() {
                return Err(RestoreError::Calendar(format!(
                    "non-finite {}",
                    crate::clock::CALENDAR_GLOBALS[index]
                )));
            }
        }
        let [year, month, day, hour, days, scale] = calendar;
        let integral = |v: f32| v.fract() == 0.0;
        if !integral(year)
            || !(0.0..u32::MAX as f32).contains(&year)
            || !integral(month)
            || !(0.0..12.0).contains(&month)
            || !integral(day)
            || day < 1.0
            || day > crate::clock::DAYS_IN_MONTH[month as usize] as f32
            || !(0.0..24.0).contains(&hour)
            || days < 0.0
            || scale < 0.0
        {
            return Err(RestoreError::Calendar(
                "invalid date, hour, elapsed days or timescale".into(),
            ));
        }
        Ok(())
    }
}
