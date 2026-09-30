// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved FormIDs remapped through plugin names into the live form table.
//! RefID and regular/light slot rules follow UESP Save File Format and
//! FallrimTools RefID.java and PluginInfo.java. This module applies the global
//! variable table. Other world state and Papyrus bindings remain separate.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{GlobalFormId, LoadOrder, Slot, Tag};
use soe_papyrus::ObjectId;
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
    #[error("cannot restore activator {id}: {reason}")]
    Activator { id: GlobalFormId, reason: String },
    #[error("cannot restore item record {id}: {reason}")]
    Item { id: GlobalFormId, reason: String },
    #[error("cannot restore BOOK, NOTE or KEYM {id}: {reason}")]
    BookNoteKey { id: GlobalFormId, reason: String },
    #[error("cannot restore reference {id}: {reason}")]
    WorldReference { id: GlobalFormId, reason: String },
    #[error("saved Papyrus block is missing")]
    PapyrusBlock,
    #[error("cannot restore Papyrus execution: {0}")]
    PapyrusExecution(String),
    #[error("saved Papyrus instance {instance:#x} targets missing form {form:?}")]
    PapyrusTarget { instance: u64, form: GlobalFormId },
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

impl crate::Game {
    /// Import saved Papyrus state, binding each script instance to its live
    /// form through the save's plugin names and RefID table.
    ///
    /// Reference-only Papyrus objects are recreated as VM objects using their
    /// saved runtime type. The returned maps retain saved IDs for stack import.
    ///
    /// # Errors
    /// The save has no valid Papyrus block, a referenced plugin or form is
    /// unavailable, or the saved Papyrus state is incompatible with the VM.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn restore_vanilla_papyrus(
        &mut self,
        save: &Save,
    ) -> Result<soe_save::papyrus::restore::RestoredInstances, RestoreError> {
        let papyrus = save.papyrus().ok_or(RestoreError::PapyrusBlock)??;
        self.restore_saved_papyrus(save, &papyrus)
    }

    /// Atomically restore saved Papyrus instances and schedule active frames.
    /// Queued messages and suspended call tables stay inert.
    ///
    /// # Errors
    ///
    /// Returns an error if the save cannot be mapped to this load order or
    /// any active frame is incompatible with the loaded scripts.
    pub fn restore_vanilla_papyrus_execution(
        &mut self,
        save: &Save,
    ) -> Result<Vec<soe_papyrus::ThreadId>, RestoreError> {
        let papyrus = save.papyrus().ok_or(RestoreError::PapyrusBlock)??;
        let before = self
            .vm
            .save_state()
            .map_err(|error| RestoreError::PapyrusExecution(error.to_string()))?;
        let result = self
            .restore_saved_papyrus(save, &papyrus)
            .and_then(|restored| {
                papyrus
                    .restore_execution(&mut self.vm, &restored)
                    .map_err(RestoreError::Save)
            });
        match result {
            Ok(threads) => Ok(threads),
            Err(error) => {
                self.vm.load_state(&before).map_err(|rollback| {
                    RestoreError::PapyrusExecution(format!(
                        "{error}; VM rollback failed: {rollback}"
                    ))
                })?;
                Err(error)
            }
        }
    }

    fn restore_saved_papyrus(
        &mut self,
        save: &Save,
        papyrus: &soe_save::papyrus::Papyrus,
    ) -> Result<soe_save::papyrus::restore::RestoredInstances, RestoreError> {
        let mapping = SavedFormMap::new(save, self.world().order())?;
        let mut saved_ids = BTreeSet::new();
        for id in papyrus
            .instances
            .iter()
            .map(|instance| instance.id)
            .chain(papyrus.references.iter().map(|reference| reference.id))
        {
            if !saved_ids.insert(id) {
                return Err(RestoreError::Save(soe_save::Error::Papyrus {
                    reason: format!("duplicate saved Papyrus object ID {id:#x}"),
                }));
            }
        }
        let reference_data: BTreeMap<_, _> = papyrus
            .reference_data
            .iter()
            .map(|data| (data.id, data))
            .collect();
        let mut reference_types = BTreeMap::new();
        for reference in &papyrus.references {
            let data = reference_data.get(&reference.id).ok_or_else(|| {
                RestoreError::Save(soe_save::Error::Papyrus {
                    reason: format!("missing variable data for reference {:#x}", reference.id),
                })
            })?;
            let kind =
                papyrus
                    .string(data.state_or_type)
                    .ok_or_else(|| soe_save::Error::Papyrus {
                        reason: format!("invalid type string for reference {:#x}", reference.id),
                    })?;
            reference_types.insert(reference.id, kind.to_owned());
        }
        let mut unbound_instances = BTreeMap::new();
        for instance in papyrus
            .instances
            .iter()
            .filter(|instance| instance.unknown == u16::MAX)
        {
            let script = papyrus
                .scripts
                .iter()
                .find(|definition| definition.name == instance.script)
                .and_then(|definition| papyrus.string(definition.name))
                .ok_or_else(|| soe_save::Error::Papyrus {
                    reason: format!("invalid script for saved instance {:#x}", instance.id),
                })?;
            unbound_instances.insert(instance.id, script.to_owned());
        }
        let mut objects = BTreeMap::new();
        {
            let mut world = self.world.borrow_mut();
            for instance in &papyrus.instances {
                if let Some(script) = unbound_instances.get(&instance.id) {
                    let object = self
                        .vm
                        .create_object_of(&format!("saved instance {:#x}", instance.id), script);
                    insert_papyrus_object(&mut objects, instance.id, Some(object))?;
                    continue;
                }
                let form = mapping
                    .reference(instance.form)?
                    .ok_or(RestoreError::Reference(instance.form.raw()))?;
                if !world.exists(form) {
                    return Err(RestoreError::PapyrusTarget {
                        instance: instance.id,
                        form,
                    });
                }
                let object = world.object_of(crate::Handle::Form(form), &mut |name, class| {
                    self.vm.create_object_of(name, class)
                });
                insert_papyrus_object(&mut objects, instance.id, Some(object))?;
            }
        }
        for reference in &papyrus.references {
            let object = self.vm.create_object_of(
                &format!("saved reference {:#x}", reference.id),
                &reference_types[&reference.id],
            );
            insert_papyrus_object(&mut objects, reference.id, Some(object))?;
        }
        papyrus
            .restore_instances(&mut self.vm, &objects)
            .map_err(|error| {
                RestoreError::Save(soe_save::Error::Papyrus {
                    reason: error.to_string(),
                })
            })
    }
}

fn insert_papyrus_object(
    objects: &mut BTreeMap<u64, Option<ObjectId>>,
    saved_id: u64,
    object: Option<ObjectId>,
) -> Result<(), RestoreError> {
    if objects.insert(saved_id, object).is_some() {
        return Err(RestoreError::Save(soe_save::Error::Papyrus {
            reason: format!("duplicate saved Papyrus object ID {saved_id:#x}"),
        }));
    }
    Ok(())
}

#[cfg(test)]
mod sav_04_tests {
    use super::*;
    use soe_esm::{plugin::FileHeader, Plugin, Record};
    use soe_papyrus::asm::ScriptBuilder;
    use soe_save::{Compression, Header};

    fn fixture_save(raw_form: u32) -> Save {
        let header = Header {
            version: 12,
            save_number: 1,
            player_name: WString::from_text("Prisoner"),
            player_level: 1,
            player_location: WString::default(),
            game_date: WString::default(),
            player_race: WString::default(),
            player_sex: 0,
            current_experience: 0.0,
            needed_experience: 0.0,
            filetime: 0,
            screenshot_width: 0,
            screenshot_height: 0,
            compression: Compression::None,
            screenshot: vec![],
        };
        let mut bytes = Vec::new();
        header.write(&mut bytes);
        let mut plugins = vec![1];
        WString::from_text("Skyrim.esm").write(&mut plugins);
        plugins.extend_from_slice(&0_u16.to_le_bytes());
        bytes.push(78);
        bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&plugins);
        let offset = bytes.len() as u32 + 100;
        soe_save::FileLocationTable {
            form_id_array_count_offset: offset + 8,
            unknown_table_3_offset: offset + 16,
            table_1_offset: offset,
            table_2_offset: offset,
            change_forms_offset: offset,
            table_3_offset: offset,
            table_1_count: 0,
            table_2_count: 0,
            table_3_count: 0,
            change_form_count: 0,
            unused: [0; 15],
        }
        .write(&mut bytes);
        GlobalData {
            kind: 1000,
            data: vec![],
        }
        .write(&mut bytes);
        bytes.extend_from_slice(&[0; 8]);
        let mut save = Save::parse(&bytes).expect("minimal save fixture parses");
        save.plugins = vec![WString::from_text("Skyrim.esm")];
        save.form_id_array = vec![raw_form];
        save
    }

    fn game() -> crate::Game {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", plugin).expect("plugin adds");
        crate::Game::new(order)
    }

    fn papyrus_bytes(bound_unknown: u16, reference: [u8; 3]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        for value in ["BoundScript", "", "Saved"] {
            WString::from_text(value).write(&mut bytes);
        }
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&41_u64.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&bound_unknown.to_le_bytes());
        bytes.extend_from_slice(&reference);
        bytes.push(0);
        for _ in 0..2 {
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&41_u64.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..6 {
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes
    }

    fn papyrus_save(raw_form: u32, bound_unknown: u16, reference: [u8; 3]) -> Save {
        let mut save = fixture_save(raw_form);
        save.table_3 = vec![GlobalData {
            kind: GlobalData::PAPYRUS,
            data: papyrus_bytes(bound_unknown, reference),
        }];
        save
    }

    #[test]
    fn sav_04_imported_instances_bind_to_live_forms_without_mapping() {
        let mut game = game();
        let bytes = ScriptBuilder::new("BoundScript", "")
            .auto_state("Saved")
            .bytes();
        game.load_script(&bytes).expect("saved script loads");
        let save = papyrus_save(0x800, 0, [0, 0, 1]);
        let imported = game.restore_vanilla_papyrus(&save).expect("instances bind");
        assert_eq!(imported.instances.len(), 1);
        let object = game.object_of_form(GlobalFormId {
            file: 0,
            object: 0x800,
        });
        assert_eq!(
            game.world().form_of(object),
            Some(GlobalFormId {
                file: 0,
                object: 0x800
            })
        );
    }

    #[test]
    fn sav_04_missing_instance_form_is_rejected_before_object_creation() {
        let mut game = game();
        game.load_script(
            &ScriptBuilder::new("BoundScript", "")
                .auto_state("Saved")
                .bytes(),
        )
        .expect("saved script loads");
        let save = papyrus_save(0x900, 0, [0, 0, 1]);
        let error = game.restore_vanilla_papyrus(&save).unwrap_err();
        assert!(matches!(
            error,
            RestoreError::PapyrusTarget { instance: 41, .. }
        ));
        assert_eq!(
            game.world().object_id(crate::Handle::Form(GlobalFormId {
                file: 0,
                object: 0x900
            })),
            None
        );
    }

    #[test]
    fn sav_04_ignores_refid_for_unbound_instances() {
        let mut game = game();
        game.load_script(
            &ScriptBuilder::new("BoundScript", "")
                .auto_state("Saved")
                .bytes(),
        )
        .expect("saved script loads");
        let save = papyrus_save(0x800, u16::MAX, [0xC0, 0, 1]);
        let imported = game
            .restore_vanilla_papyrus(&save)
            .expect("unbound instance ignores its RefID");
        assert_eq!(imported.instances.len(), 1);
        assert_eq!(
            game.world().object_id(crate::Handle::Form(GlobalFormId {
                file: 0,
                object: 0x800
            })),
            None
        );
    }
}
