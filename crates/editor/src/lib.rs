// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

use soe_esm::records::gameplay::{Scene as GameplayScene, SceneActionData};
use soe_esm::records::navm::{DoorLink, Geometry, NavMesh, Parent, Triangle};
use soe_esm::{Entry, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Subrecord, Tag};
use soe_navmesh::{Agent, Scene as NavmeshScene};
use soe_papyrus::{pex, psc};
use std::collections::BTreeSet;
use thiserror::Error;

/// Errors produced while opening or editing a plugin document.
#[derive(Debug, Error)]
pub enum EditorError {
    /// A plugin could not be read from disk.
    #[error("could not read plugin {path}: {source}")]
    Io {
        /// Path to the plugin.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A plugin could not be parsed or added to the load order.
    #[error("plugin {name}: {message}")]
    Plugin { name: String, message: String },
    /// The active plugin is not part of the opened load order.
    #[error("active plugin {0} is not in the load order")]
    MissingActivePlugin(String),
    /// The requested form has no winning record.
    #[error("form {0} has no winning record")]
    MissingForm(GlobalFormId),
    /// The active plugin cannot represent this record safely yet.
    #[error("cannot edit form {form}: {reason}")]
    UnsupportedEdit {
        /// Form being edited.
        form: GlobalFormId,
        /// Limitation that prevented the edit.
        reason: &'static str,
    },
    /// A record contains multiple occurrences of the requested subrecord.
    #[error("form {form} has multiple {tag} subrecords")]
    RepeatedSubrecord {
        /// Form being edited.
        form: GlobalFormId,
        /// Subrecord tag that was repeated.
        tag: Tag,
    },
    /// Papyrus compilation or VMAD binding failed.
    #[error("Papyrus script could not be attached: {0}")]
    Papyrus(String),
    /// A NAVM edit would create invalid geometry or indices.
    #[error("cannot edit navmesh {form}: {reason}")]
    InvalidNavmesh {
        /// Navmesh being edited.
        form: GlobalFormId,
        /// Why the requested geometry was rejected.
        reason: &'static str,
    },
    /// A typed field value cannot be represented in its plugin subrecord.
    #[error("invalid value for {field} on form {form}: {reason}")]
    InvalidFieldValue {
        /// Form being edited.
        form: GlobalFormId,
        /// Typed field being edited.
        field: &'static str,
        /// Reason the value cannot be represented.
        reason: &'static str,
    },
    /// A quest subrecord has an invalid shape for the requested field.
    #[error("quest form {form} has invalid {tag} data: {reason}")]
    InvalidQuestField {
        /// Quest being edited.
        form: GlobalFormId,
        /// Field whose layout is invalid.
        tag: Tag,
        /// Why the field could not be edited.
        reason: &'static str,
    },
    /// The quest record stores localized string IDs that need a string-table writer.
    #[error(
        "quest form {0} uses localized strings, which cannot be updated by the plugin-only writer"
    )]
    LocalizedQuestText(GlobalFormId),
    /// The requested response is absent from an INFO record.
    #[error("form {form} has no response at index {response}")]
    MissingResponse {
        /// INFO form being edited.
        form: GlobalFormId,
        /// Zero-based response index.
        response: usize,
    },
    /// A scene edit would create an invalid record or link.
    #[error("cannot edit scene {form}: {reason}")]
    InvalidScene {
        /// Scene being edited.
        form: GlobalFormId,
        /// Reason the scene edit was refused.
        reason: &'static str,
    },
    /// A condition list contains tags or data that are not a valid CTDA sequence.
    #[error("quest form {form} has an invalid alias condition list: {reason}")]
    InvalidQuestConditions {
        /// Quest being edited.
        form: GlobalFormId,
        /// Why the condition list is invalid.
        reason: &'static str,
    },
    /// Recast could not regenerate the requested navmesh.
    #[error("cannot regenerate navmesh {form}: {message}")]
    NavmeshGeneration {
        /// Navmesh being regenerated.
        form: GlobalFormId,
        /// Generation failure.
        message: String,
    },
    /// Saving the active plugin failed.
    #[error("could not write plugin {path}: {source}")]
    Save {
        /// Destination path.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
}

/// The typed model path on a winning STAT record and the plugin that supplies
/// that record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatModelField {
    /// NIF path from the STAT `MODL` field, when present.
    pub value: Option<String>,
    /// File name of the plugin that currently supplies the winning record.
    pub winning_source: String,
}

/// One version of a form in a selected load order.
pub struct OverrideVersion<'a> {
    /// Plugin that defines this version.
    pub plugin_name: &'a str,
    /// Record data authored by the plugin.
    pub record: &'a Record,
    /// Whether this is the version used by the load order.
    pub is_winner: bool,
}

/// An opened load order and its editable active plugin.
pub struct EditorDocument {
    load_order: LoadOrder,
    active_name: String,
    active_index: usize,
    active_plugin: Plugin,
    undo: Vec<Plugin>,
}

impl EditorDocument {
    /// Open plugins in load-order sequence and select the active plugin.
    ///
    /// # Errors
    /// Returns an error if a file cannot be read or parsed, if the load order
    /// is invalid, or if `active_name` is not among the supplied paths.
    pub fn open(paths: &[PathBuf], active_name: &str) -> Result<Self, EditorError> {
        let mut load_order = LoadOrder::new();
        let mut active_index = None;
        for path in paths {
            let name = file_name(path)?;
            let bytes = fs::read(path).map_err(|source| EditorError::Io {
                path: path.clone(),
                source,
            })?;
            let plugin = Plugin::parse(&bytes).map_err(|error| EditorError::Plugin {
                name: name.clone(),
                message: error.to_string(),
            })?;
            let index = load_order
                .add(&name, plugin)
                .map_err(|error| EditorError::Plugin {
                    name: name.clone(),
                    message: error.to_string(),
                })?;
            if name.eq_ignore_ascii_case(active_name) {
                active_index = Some(index);
            }
        }
        let active_index = active_index
            .ok_or_else(|| EditorError::MissingActivePlugin(active_name.to_string()))?;
        let active_name = load_order.file(active_index).name.clone();
        let active_plugin = load_order.file(active_index).plugin.clone();
        Ok(Self {
            load_order,
            active_name,
            active_index,
            active_plugin,
            undo: Vec::new(),
        })
    }

    /// Return the opened load order.
    pub fn load_order(&self) -> &LoadOrder {
        &self.load_order
    }

    /// Return every version of a selected form in load-order sequence.
    ///
    /// Earlier versions are marked as losing and the final version is marked
    /// as winning. A form without overrides has one winning version. An
    /// unknown form produces an empty chain.
    pub fn override_chain(&self, form: GlobalFormId) -> Vec<OverrideVersion<'_>> {
        let overrides = self.load_order.overrides(form);
        overrides
            .iter()
            .enumerate()
            .map(|(index, record_ref)| OverrideVersion {
                plugin_name: &self.load_order.file(record_ref.file).name,
                record: self.load_order.record(*record_ref),
                is_winner: index + 1 == overrides.len(),
            })
            .collect()
    }

    /// Search winning forms by optional record signature and editor ID text.
    ///
    /// An empty query matches every winning form that passes the signature
    /// filter. A non-empty query matches an ASCII case-insensitive substring
    /// of the record's `EDID`. The iterator yields form IDs only. Callers can
    /// retrieve the winning record when a result is selected instead of
    /// creating one Godot resource for every form in the load order.
    pub fn search_forms<'a>(
        &'a self,
        tag: Option<Tag>,
        query: &'a str,
    ) -> impl Iterator<Item = GlobalFormId> + 'a {
        self.load_order
            .winners()
            .filter_map(move |(id, record_ref)| {
                let record = self.load_order.record(record_ref);
                if tag.is_some_and(|expected| record.tag != expected) {
                    return None;
                }
                let matches = query.is_empty()
                    || record
                        .find(Tag(*b"EDID"))
                        .is_some_and(|editor_id| contains_ascii_case_insensitive(editor_id, query));
                matches.then_some(id)
            })
    }

    /// Return the active plugin's file name.
    pub fn active_name(&self) -> &str {
        &self.active_name
    }

    /// Return the active plugin being edited.
    pub fn active_plugin(&self) -> &Plugin {
        &self.active_plugin
    }

    /// Inspect the winning STAT model field through the typed ESM view.
    ///
    /// The source is the plugin that supplies the winning record, including
    /// after an override is created in the active plugin.
    ///
    /// # Errors
    /// Returns an error if the form is missing, is not a STAT record, or its
    /// typed fields are malformed.
    pub fn stat_model_field(&self, form: GlobalFormId) -> Result<StatModelField, EditorError> {
        let record_ref = self
            .load_order
            .winner_ref(form)
            .ok_or(EditorError::MissingForm(form))?;
        let record = self.load_order.record(record_ref);
        if record.tag != Tag(*b"STAT") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the typed model field is available only for STAT records",
            });
        }
        let typed = soe_esm::records::stat::Static::from_record(record).map_err(|error| {
            EditorError::Plugin {
                name: self.load_order.file(record_ref.file).name.clone(),
                message: error.to_string(),
            }
        })?;
        Ok(StatModelField {
            value: typed.model,
            winning_source: self.load_order.file(record_ref.file).name.clone(),
        })
    }

    /// Set a STAT model path, creating an active-plugin override as needed.
    ///
    /// This is a typed field edit. The path is stored as the NUL-terminated
    /// `MODL` string used by the plugin record, and undo restores the prior
    /// active plugin and winning source.
    ///
    /// # Errors
    /// Returns an error if the form is missing, is not a STAT record, its
    /// typed fields are malformed, or the value contains a NUL byte.
    pub fn set_stat_model_field(
        &mut self,
        form: GlobalFormId,
        value: &str,
    ) -> Result<(), EditorError> {
        let record_ref = self
            .load_order
            .winner_ref(form)
            .ok_or(EditorError::MissingForm(form))?;
        let record = self.load_order.record(record_ref);
        if record.tag != Tag(*b"STAT") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the typed model field is available only for STAT records",
            });
        }
        soe_esm::records::stat::Static::from_record(record).map_err(|error| {
            EditorError::Plugin {
                name: self.load_order.file(record_ref.file).name.clone(),
                message: error.to_string(),
            }
        })?;
        if value.as_bytes().contains(&0) {
            return Err(EditorError::InvalidFieldValue {
                form,
                field: "STAT.MODL",
                reason: "model paths cannot contain NUL bytes",
            });
        }
        let mut encoded = value.as_bytes().to_vec();
        encoded.push(0);
        self.set_subrecord(form, Tag(*b"MODL"), encoded)
    }

    /// Replace or append one subrecord, creating an active-plugin override.
    ///
    /// This milestone accepts top-level STAT and GLOB records only. Existing
    /// linked fields are copied as authored, so forms requiring FormID
    /// remapping are rejected until the ESM renumbering work is available.
    ///
    /// # Errors
    /// Returns an error if the form is missing, the winning record is nested,
    /// its owner is not a master of the active plugin, or the field repeats.
    pub fn set_subrecord(
        &mut self,
        form: GlobalFormId,
        tag: Tag,
        data: Vec<u8>,
    ) -> Result<(), EditorError> {
        let existing = self.active_record(form).cloned();
        let (mut record, add_override) = match existing {
            Some(record) => (record, false),
            None => {
                let record_ref = self
                    .load_order
                    .winner_ref(form)
                    .ok_or(EditorError::MissingForm(form))?;
                let source = self.load_order.record(record_ref);
                let source_plugin = &self.load_order.file(record_ref.file).plugin;
                let dialogue_topic =
                    source.tag == Tag(*b"DIAL") && is_top_level(source_plugin, record_ref.record);
                let dialogue_info = source.tag == Tag(*b"INFO")
                    && matches!(source.placement.group, Some(GroupKind::TopicChildren(_)));
                if !dialogue_topic
                    && !dialogue_info
                    && !is_top_level(source_plugin, record_ref.record)
                {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "nested record placement needs FMT-15 and FMT-26 FormID remapping",
                    });
                }
                if source.tag != Tag(*b"STAT")
                    && source.tag != Tag(*b"GLOB")
                    && !dialogue_topic
                    && !dialogue_info
                {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "record reference relocation is not available before FMT-15",
                    });
                }
                let mut record = source.clone();
                record.form_id = self.local_form_id(form)?;
                if dialogue_topic {
                    self.remap_dialogue_topic_links(&mut record, record_ref.file, form)?;
                }
                (record, true)
            }
        };

        let mut matching = record
            .subrecords
            .iter_mut()
            .filter(|subrecord| subrecord.tag == tag);
        if let Some(subrecord) = matching.next() {
            if matching.next().is_some() {
                return Err(EditorError::RepeatedSubrecord { form, tag });
            }
            subrecord.data = data;
        } else {
            record.subrecords.push(Subrecord::new(tag, data));
        }

        self.undo.push(self.active_plugin.clone());
        if add_override {
            if record.tag == Tag(*b"INFO") {
                self.push_dialogue_info(
                    record,
                    self.load_order
                        .winner_ref(form)
                        .map_or(self.active_index, |record_ref| record_ref.file),
                )?;
            } else {
                self.active_plugin.push_top(record);
            }
        } else if let Some(index) = self.active_record_index(form) {
            self.active_plugin.records[index] = record;
        }
        self.sync_record_count();
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Compile a PSC source and bind its declared auto properties to a record.
    ///
    /// The returned bytes are the Skyrim PEX output. This editor milestone
    /// writes scalar property bindings into an otherwise empty VMAD subrecord.
    ///
    /// # Errors
    /// Returns an error for unsupported source, invalid ownership, records
    /// without an existing active-plugin override, or records with VMAD data.
    pub fn compile_script_for_form(
        &mut self,
        form: GlobalFormId,
        source_name: &str,
        source: &str,
    ) -> Result<Vec<u8>, EditorError> {
        let pex_bytes = psc::compile(source_name, source)
            .map_err(|error| EditorError::Papyrus(error.to_string()))?;
        let script =
            pex::parse(&pex_bytes).map_err(|error| EditorError::Papyrus(error.to_string()))?;
        let object = script
            .objects
            .first()
            .ok_or_else(|| EditorError::Papyrus("compiled PEX has no object".into()))?;
        let vmad = encode_vmad(object)?;

        let record = self
            .load_order
            .winner(form)
            .ok_or(EditorError::MissingForm(form))?;
        if record.find(Tag(*b"VMAD")).is_some() {
            return Err(EditorError::Papyrus(
                "record already has VMAD data, preserving existing bindings is not supported"
                    .into(),
            ));
        }
        self.set_subrecord(form, Tag(*b"VMAD"), vmad)?;
        Ok(pex_bytes)
    }

    /// Compile and bind a script, then save the edited plugin.
    ///
    /// Returns the compiled PEX bytes and the plugin parsed from the written
    /// output.
    ///
    /// # Errors
    /// Returns an error if compilation, binding, or plugin output fails.
    pub fn save_with_script(
        &mut self,
        form: GlobalFormId,
        source_name: &str,
        source: &str,
        plugin_path: impl AsRef<Path>,
    ) -> Result<(Vec<u8>, Plugin), EditorError> {
        let pex_bytes = self.compile_script_for_form(form, source_name, source)?;
        let plugin = self.save(plugin_path)?;
        Ok((pex_bytes, plugin))
    }

    /// Edit one existing quest stage log entry by stage and entry occurrence.
    ///
    /// Occurrence indices preserve distinct entries when stage numbers repeat.
    ///
    /// # Errors
    /// Returns an error if the form is not a supported QUST record, its layout
    /// is malformed, the selected entry has no CNAM text, or the quest uses
    /// localized string IDs.
    pub fn edit_quest_stage_log_entry_text(
        &mut self,
        form: GlobalFormId,
        stage_occurrence: usize,
        entry_occurrence: usize,
        text: &str,
    ) -> Result<(), EditorError> {
        let mut record = self.quest_record_for_edit(form)?;
        reject_localized_quest_text(form, &record)?;
        let subrecord_index =
            quest_stage_log_text_index(form, &record, stage_occurrence, entry_occurrence)?;
        record.subrecords[subrecord_index].data = nul_terminated(text);
        self.replace_edited_record(form, record)
    }

    /// Edit one existing quest objective's display text by occurrence.
    ///
    /// Occurrence indices preserve distinct objectives when objective numbers
    /// repeat. Target aliases and conditions remain in their authored order.
    ///
    /// # Errors
    /// Returns an error if the form is not a supported QUST record, its layout
    /// is malformed, the selected objective has no NNAM text, or the quest
    /// uses localized string IDs.
    pub fn edit_quest_objective_text(
        &mut self,
        form: GlobalFormId,
        objective_occurrence: usize,
        text: &str,
    ) -> Result<(), EditorError> {
        let mut record = self.quest_record_for_edit(form)?;
        reject_localized_quest_text(form, &record)?;
        let subrecord_index = quest_objective_text_index(form, &record, objective_occurrence)?;
        record.subrecords[subrecord_index].data = nul_terminated(text);
        self.replace_edited_record(form, record)
    }

    fn quest_record_for_edit(&self, form: GlobalFormId) -> Result<Record, EditorError> {
        let record_ref = self
            .load_order
            .winner_ref(form)
            .ok_or(EditorError::MissingForm(form))?;
        let source = self.load_order.record(record_ref);
        if source.tag != Tag(*b"QUST") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "quest text editing requires a QUST record",
            });
        }
        let active = self.active_record(form).cloned();
        if active.is_some() {
            return active.ok_or(EditorError::MissingForm(form));
        }
        if !is_top_level(
            &self.load_order.file(record_ref.file).plugin,
            record_ref.record,
        ) {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "nested record placement needs FMT-15 and FMT-26 FormID remapping",
            });
        }
        if !quest_master_order_is_aligned(&self.load_order, self.active_index, record_ref.file) {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "quest reference relocation needs matching source and active master order",
            });
        }
        let mut record = source.clone();
        record.form_id = self.local_form_id(form)?;
        Ok(record)
    }

    fn replace_edited_record(
        &mut self,
        form: GlobalFormId,
        record: Record,
    ) -> Result<(), EditorError> {
        self.undo.push(self.active_plugin.clone());
        if let Some(index) = self.active_record_index(form) {
            self.active_plugin.records[index] = record;
        } else {
            self.active_plugin.push_top(record);
        }
        self.sync_record_count();
        self.rebuild_load_order()
    }

    /// Replace the text of one response in an INFO record.
    ///
    /// Responses are selected by the order of their TRDT subrecords. Their
    /// following NAM1 fields remain attached to the same response.
    ///
    /// # Errors
    /// Returns an error if the INFO form or response is missing, the INFO
    /// record is not in a topic group, or the active plugin cannot represent
    /// the override.
    pub fn set_dialogue_response_text(
        &mut self,
        form: GlobalFormId,
        response_index: usize,
        data: Vec<u8>,
    ) -> Result<(), EditorError> {
        let active_index = self.active_record_index(form);
        let record_ref = self
            .load_order
            .winner_ref(form)
            .ok_or(EditorError::MissingForm(form))?;
        let mut record = active_index
            .and_then(|index| self.active_plugin.records.get(index).cloned())
            .unwrap_or_else(|| self.load_order.record(record_ref).clone());
        if record.tag != Tag(*b"INFO") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "response text can only be edited on INFO records",
            });
        }
        let mut response = 0;
        let mut response_text = None;
        for (index, subrecord) in record.subrecords.iter().enumerate() {
            if subrecord.tag == Tag(*b"TRDT") {
                if response == response_index {
                    response_text = record.subrecords[index + 1..]
                        .iter()
                        .take_while(|next| next.tag != Tag(*b"TRDT"))
                        .position(|next| next.tag == Tag(*b"NAM1"))
                        .map(|offset| index + 1 + offset);
                    break;
                }
                response += 1;
            }
        }
        let text_index = response_text.ok_or(EditorError::MissingResponse {
            form,
            response: response_index,
        })?;
        record.subrecords[text_index].data = data;

        self.undo.push(self.active_plugin.clone());
        if let Some(index) = active_index {
            self.active_plugin.records[index] = record;
        } else {
            let source_file = record_ref.file;
            record.form_id = self.local_form_id(form)?;
            self.push_dialogue_info(record, source_file)?;
        }
        self.sync_record_count();
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Apply one circular LAND height brush dab as a single undoable edit.
    ///
    /// The LAND record must already be present in the active plugin. `center`
    /// and `radius` use cell-local world units, and `strength` is the height
    /// change at the brush centre. The operation updates VHGT and derived
    /// VNML together while preserving the record's other subrecords.
    ///
    /// # Errors
    /// Returns an error if the form is missing, is not a LAND record in the
    /// active plugin, or its height data cannot be edited.
    pub fn brush_land_heights(
        &mut self,
        form: GlobalFormId,
        center: [f32; 2],
        radius: f32,
        strength: f32,
    ) -> Result<(), EditorError> {
        let Some(index) = self.active_record_index(form) else {
            return Err(EditorError::MissingForm(form));
        };
        if self.active_plugin.records[index].tag != Tag(*b"LAND") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "height brush requires an active-plugin LAND record",
            });
        }
        let mut record = self.active_plugin.records[index].clone();
        soe_esm::records::land::apply_height_brush(&mut record, center, radius, strength).map_err(
            |error| EditorError::Plugin {
                name: self.active_name.clone(),
                message: error.to_string(),
            },
        )?;

        self.undo.push(self.active_plugin.clone());
        self.active_plugin.records[index] = record;
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Apply a gizmo transform to a placed reference in the active plugin.
    ///
    /// The `REFR` must already belong to the active plugin. This keeps the
    /// record in its existing cell group while the transform remains the
    /// single source used by rendering and plugin output.
    ///
    /// # Errors
    /// Returns an error if the form is missing, is not an active-plugin
    /// reference, has malformed or repeated `DATA`, or the load order cannot
    /// be rebuilt.
    pub fn set_reference_transform(
        &mut self,
        form: GlobalFormId,
        transform: soe_esm::records::Transform,
    ) -> Result<(), EditorError> {
        let Some(index) = self.active_record_index(form) else {
            return Err(EditorError::MissingForm(form));
        };
        let record = self
            .active_plugin
            .records
            .get(index)
            .ok_or(EditorError::MissingForm(form))?;
        if record.tag != Tag(*b"REFR") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the selected form is not a placed reference",
            });
        }
        let mut data = record
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.tag == Tag(*b"DATA"));
        let Some(existing) = data.next() else {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the placed reference has no transform DATA",
            });
        };
        if data.next().is_some() {
            return Err(EditorError::RepeatedSubrecord {
                form,
                tag: Tag(*b"DATA"),
            });
        }
        if existing.data.len() != 24 {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the placed reference transform DATA is not 24 bytes",
            });
        }

        let mut replacement = record.clone();
        if let Some(subrecord) = replacement
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.tag == Tag(*b"DATA"))
        {
            subrecord.data = transform.to_bytes().to_vec();
        }
        self.undo.push(self.active_plugin.clone());
        self.active_plugin.records[index] = replacement;
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Set or clear a quest alias's force-into-alias fill rule.
    ///
    /// `alias_occurrence` selects the ALST alias in record order. A value of
    /// `None` removes its optional ALFI field.
    ///
    /// # Errors
    /// Returns an error if the quest or alias is unsupported or its ALFI field
    /// is malformed or repeated.
    pub fn set_quest_alias_fill_rule(
        &mut self,
        form: GlobalFormId,
        alias_occurrence: usize,
        target_alias: Option<i32>,
    ) -> Result<(), EditorError> {
        let mut record = self.quest_record_for_edit(form)?;
        let (start, end) = quest_alias_range(form, &record, alias_occurrence)?;
        let fields = (start + 1..end)
            .filter(|index| record.subrecords[*index].tag == Tag(*b"ALFI"))
            .collect::<Vec<_>>();
        if fields.len() > 1 {
            return Err(EditorError::InvalidQuestField {
                form,
                tag: Tag(*b"ALFI"),
                reason: "alias has repeated fill rules",
            });
        }
        if fields
            .first()
            .is_some_and(|index| record.subrecords[*index].data.len() != 4)
        {
            return Err(EditorError::InvalidQuestField {
                form,
                tag: Tag(*b"ALFI"),
                reason: "fill rule must contain four bytes",
            });
        }
        match (fields.first().copied(), target_alias) {
            (Some(index), Some(alias)) => {
                record.subrecords[index].data = alias.to_le_bytes().to_vec();
            }
            (Some(index), None) => {
                record.subrecords.remove(index);
            }
            (None, Some(alias)) => {
                let after_flags = (start + 1..end)
                    .find(|index| record.subrecords[*index].tag == Tag(*b"FNAM"))
                    .ok_or(EditorError::InvalidQuestField {
                        form,
                        tag: Tag(*b"FNAM"),
                        reason: "alias flags are missing before the fill rule",
                    })?;
                record.subrecords.insert(
                    after_flags + 1,
                    Subrecord::new(Tag(*b"ALFI"), alias.to_le_bytes()),
                );
            }
            (None, None) => {}
        }
        self.replace_edited_record(form, record)
    }

    /// Replace one quest alias's ordered match-condition subrecords.
    ///
    /// The supplied list may contain CTDA records with optional CIS1 and CIS2
    /// strings. Other alias fields retain their existing order and bytes.
    ///
    /// # Errors
    /// Returns an error if the quest or alias is unsupported, or if the input
    /// condition list is malformed.
    pub fn set_quest_alias_conditions(
        &mut self,
        form: GlobalFormId,
        alias_occurrence: usize,
        conditions: &[Subrecord],
    ) -> Result<(), EditorError> {
        let mut record = self.quest_record_for_edit(form)?;
        let (start, end) = quest_alias_range(form, &record, alias_occurrence)?;
        validate_alias_conditions(form, &record, conditions)?;
        let mut insertion = None;
        let mut retained = Vec::with_capacity(record.subrecords.len() + conditions.len());
        for (index, subrecord) in record.subrecords.drain(..).enumerate() {
            if index > start && index < end && is_condition_subrecord(subrecord.tag) {
                insertion.get_or_insert(retained.len());
            } else {
                retained.push(subrecord);
            }
        }
        let insertion = insertion.unwrap_or_else(|| {
            let retained_aliases = retained
                .iter()
                .enumerate()
                .filter_map(|(index, subrecord)| (subrecord.tag == Tag(*b"ALST")).then_some(index))
                .collect::<Vec<_>>();
            let Some(&alias_start) = retained_aliases.get(alias_occurrence) else {
                return retained.len();
            };
            let alias_end = retained[alias_start + 1..]
                .iter()
                .position(|subrecord| subrecord.tag == Tag(*b"ALED"))
                .map_or(retained.len(), |offset| alias_start + 1 + offset);
            let alias_body = &retained[alias_start + 1..alias_end];
            alias_body
                .iter()
                .rposition(|subrecord| is_alias_fill_subrecord(subrecord.tag))
                .or_else(|| {
                    alias_body
                        .iter()
                        .rposition(|subrecord| subrecord.tag == Tag(*b"FNAM"))
                })
                .or_else(|| {
                    alias_body
                        .iter()
                        .rposition(|subrecord| subrecord.tag == Tag(*b"ALID"))
                })
                .map_or(alias_end, |offset| alias_start + 2 + offset)
        });
        retained.splice(insertion..insertion, conditions.iter().cloned());
        record.subrecords = retained;
        self.replace_edited_record(form, record)
    }

    /// Undo the most recent edit, returning whether an edit was available.
    ///
    /// # Errors
    /// Returns an error if the restored plugin no longer forms a valid load order.
    /// Rename one SCEN phase while retaining its other subrecords.
    ///
    /// # Errors
    /// Returns an error if the form is not an editable top-level SCEN, the
    /// phase index is out of range, or its links are invalid.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn set_scene_phase_name(
        &mut self,
        form: GlobalFormId,
        phase_index: usize,
        name: &str,
    ) -> Result<(), EditorError> {
        let (mut record, add_override) = self.scene_for_edit(form)?;
        let scene = GameplayScene::from_record(&record).map_err(|_| EditorError::InvalidScene {
            form,
            reason: "SCEN fields are malformed",
        })?;
        if phase_index >= scene.phases.len() {
            return Err(EditorError::InvalidScene {
                form,
                reason: "phase index is out of range",
            });
        }
        let range = scene_phase_ranges(&record)
            .get(phase_index)
            .copied()
            .ok_or(EditorError::InvalidScene {
                form,
                reason: "phase markers do not match the decoded phases",
            })?;
        set_scene_field(&mut record, range, Tag(*b"NAM0"), zstring_bytes(name), form)?;
        self.commit_scene(form, record, add_override)
    }

    /// Set a Dialogue action's topic to a resolved DIAL form.
    ///
    /// The FormID is written using the active plugin's master table. Other
    /// SCEN subrecords in the source record are preserved.
    ///
    /// # Errors
    /// Returns an error if the scene, action or topic is invalid, if the
    /// active plugin cannot represent the topic, or if scene links are bad.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn set_scene_action_topic(
        &mut self,
        form: GlobalFormId,
        action_index: usize,
        topic: GlobalFormId,
    ) -> Result<(), EditorError> {
        let topic_record = self
            .load_order
            .winner(topic)
            .ok_or(EditorError::InvalidScene {
                form,
                reason: "dialogue topic is missing from the load order",
            })?;
        if topic_record.tag != Tag(*b"DIAL") {
            return Err(EditorError::InvalidScene {
                form,
                reason: "dialogue action topic does not resolve to DIAL",
            });
        }
        let topic_raw = self
            .local_form_id(topic)
            .map_err(|_| EditorError::InvalidScene {
                form,
                reason: "active plugin does not list the dialogue topic owner as a master",
            })?;

        let (mut record, add_override) = self.scene_for_edit(form)?;
        let scene = GameplayScene::from_record(&record).map_err(|_| EditorError::InvalidScene {
            form,
            reason: "SCEN fields are malformed",
        })?;
        let action = scene
            .actions
            .get(action_index)
            .ok_or(EditorError::InvalidScene {
                form,
                reason: "action index is out of range",
            })?;
        if action.action_type != 0 {
            return Err(EditorError::InvalidScene {
                form,
                reason: "only Dialogue actions have a topic field",
            });
        }
        let range = scene_action_ranges(&record)
            .get(action_index)
            .copied()
            .ok_or(EditorError::InvalidScene {
                form,
                reason: "action markers do not match the decoded actions",
            })?;
        set_scene_field(
            &mut record,
            range,
            Tag(*b"DATA"),
            topic_raw.to_le_bytes().to_vec(),
            form,
        )?;
        self.commit_scene(form, record, add_override)
    }

    fn scene_for_edit(&self, form: GlobalFormId) -> Result<(Record, bool), EditorError> {
        if let Some(record) = self.active_record(form) {
            if record.tag != Tag(*b"SCEN") {
                return Err(EditorError::InvalidScene {
                    form,
                    reason: "record is not SCEN",
                });
            }
            self.validate_scene_links(record, self.active_index, form)?;
            return Ok((record.clone(), false));
        }

        let record_ref = self
            .load_order
            .winner_ref(form)
            .ok_or(EditorError::MissingForm(form))?;
        let source_file = record_ref.file;
        let plugin = &self.load_order.file(source_file).plugin;
        let source = self.load_order.record(record_ref);
        if source.tag != Tag(*b"SCEN") {
            return Err(EditorError::InvalidScene {
                form,
                reason: "record is not SCEN",
            });
        }
        if !is_top_level(plugin, record_ref.record) {
            return Err(EditorError::InvalidScene {
                form,
                reason: "nested SCEN records cannot be overridden safely",
            });
        }
        let mut record = source.clone();
        self.relocate_scene_links(&mut record, source_file, form)?;
        record.form_id = self
            .local_form_id(form)
            .map_err(|_| EditorError::InvalidScene {
                form,
                reason: "active plugin does not list the scene owner as a master",
            })?;
        self.validate_scene_links(&record, self.active_index, form)?;
        Ok((record, true))
    }

    fn relocate_scene_links(
        &self,
        record: &mut Record,
        source_file: usize,
        form: GlobalFormId,
    ) -> Result<(), EditorError> {
        let mut action_type = None;
        for subrecord in &mut record.subrecords {
            match subrecord.tag.as_bytes() {
                b"ANAM" if subrecord.data.is_empty() => action_type = None,
                b"ANAM" => {
                    if subrecord.data.len() != 2 {
                        return Err(EditorError::InvalidScene {
                            form,
                            reason: "action type has an invalid size",
                        });
                    }
                    action_type = Some(u16::from_le_bytes([subrecord.data[0], subrecord.data[1]]));
                }
                b"DATA" if action_type == Some(0) => {
                    relocate_scene_form_id(self, subrecord, source_file, *b"DIAL", form)?;
                }
                b"PNAM" if action_type == Some(1) => {
                    if subrecord.data.len() % 4 != 0 {
                        return Err(EditorError::InvalidScene {
                            form,
                            reason: "package list has an invalid size",
                        });
                    }
                    for bytes in subrecord.data.as_chunks_mut::<4>().0 {
                        relocate_scene_id(self, bytes, source_file, *b"PACK", form)?;
                    }
                }
                // Outside a Package action PNAM is the required parent QUST.
                b"PNAM" => {
                    relocate_scene_form_id(self, subrecord, source_file, *b"QUST", form)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_scene_links(
        &self,
        record: &Record,
        source_file: usize,
        form: GlobalFormId,
    ) -> Result<(), EditorError> {
        let scene = GameplayScene::from_record(record).map_err(|_| EditorError::InvalidScene {
            form,
            reason: "SCEN fields are malformed",
        })?;
        let quest = scene.quest.ok_or(EditorError::InvalidScene {
            form,
            reason: "parent quest is missing",
        })?;
        self.validate_scene_link(source_file, quest, *b"QUST", false, form)?;
        for action in &scene.actions {
            if action
                .start_phase
                .is_some_and(|phase| phase as usize >= scene.phases.len())
                || action
                    .end_phase
                    .is_some_and(|phase| phase as usize >= scene.phases.len())
            {
                return Err(EditorError::InvalidScene {
                    form,
                    reason: "action phase link is out of range",
                });
            }
            match &action.data {
                SceneActionData::Dialogue { topic: Some(topic) } => {
                    self.validate_scene_link(source_file, *topic, *b"DIAL", true, form)?;
                }
                SceneActionData::Package { packages } => {
                    for package in packages {
                        self.validate_scene_link(source_file, *package, *b"PACK", true, form)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_scene_link(
        &self,
        source_file: usize,
        raw: u32,
        expected_tag: [u8; 4],
        allow_null: bool,
        form: GlobalFormId,
    ) -> Result<(), EditorError> {
        if raw == 0 && allow_null {
            return Ok(());
        }
        let target = self.load_order.resolve(source_file, raw);
        let tag = self
            .load_order
            .winner(target)
            .map(|record| record.tag)
            .ok_or(EditorError::InvalidScene {
                form,
                reason: "a linked form is missing from the load order",
            })?;
        if tag != Tag(expected_tag) {
            return Err(EditorError::InvalidScene {
                form,
                reason: "a linked form has the wrong record type",
            });
        }
        Ok(())
    }

    fn commit_scene(
        &mut self,
        form: GlobalFormId,
        record: Record,
        add_override: bool,
    ) -> Result<(), EditorError> {
        self.undo.push(self.active_plugin.clone());
        if add_override {
            self.active_plugin.push_top(record);
        } else if let Some(index) = self.active_record_index(form) {
            self.active_plugin.records[index] = record;
        }
        self.sync_record_count();
        self.rebuild_load_order()
    }

    /// Undo the most recent edit, returning whether an edit was available.
    ///
    /// # Errors
    /// Returns an error if the restored plugin no longer forms a valid load order.
    pub fn undo(&mut self) -> Result<bool, EditorError> {
        let Some(previous) = self.undo.pop() else {
            return Ok(false);
        };
        self.active_plugin = previous;
        self.rebuild_load_order()?;
        Ok(true)
    }

    /// Add a triangle using three existing vertex indices.
    ///
    /// Triangle adjacency and the spatial grid are rebuilt before the record
    /// is changed. The edit is rejected if it would exceed NVNM index limits
    /// or create ambiguous edge adjacency.
    pub fn add_navmesh_triangle(
        &mut self,
        form: GlobalFormId,
        vertices: [u16; 3],
        flags: u16,
        cover_flags: u16,
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            if geometry.triangles.len() > i16::MAX as usize {
                return Err("triangle count exceeds NVNM signed index capacity");
            }
            geometry.triangles.push(Triangle {
                vertices,
                edges: [-1; 3],
                flags: flags & !0x7,
                cover_flags,
            });
            Ok(())
        })
    }

    /// Delete a triangle and remove or remap its triangle-indexed metadata.
    pub fn delete_navmesh_triangle(
        &mut self,
        form: GlobalFormId,
        triangle: usize,
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            if triangle >= geometry.triangles.len() {
                return Err("triangle index is out of range");
            }
            geometry.triangles.remove(triangle);
            for tri in &mut geometry.triangles {
                for edge in 0..3 {
                    if tri.flags & (1 << edge) != 0 {
                        continue;
                    }
                    match usize::try_from(tri.edges[edge]) {
                        Ok(target) if target == triangle => tri.edges[edge] = -1,
                        Ok(target) if target > triangle => tri.edges[edge] -= 1,
                        _ => {}
                    }
                }
            }
            geometry
                .door_links
                .retain_mut(|link| match usize::try_from(link.triangle) {
                    Ok(target) if target == triangle => false,
                    Ok(target) if target > triangle => {
                        link.triangle -= 1;
                        true
                    }
                    _ => true,
                });
            geometry
                .cover_triangles
                .retain_mut(|target| match usize::try_from(*target) {
                    Ok(index) if index == triangle => false,
                    Ok(index) if index > triangle => {
                        *target -= 1;
                        true
                    }
                    _ => true,
                });
            Ok(())
        })
    }

    /// Move one vertex and rebuild the navmesh bounds and spatial grid.
    pub fn move_navmesh_vertex(
        &mut self,
        form: GlobalFormId,
        vertex: usize,
        position: [f32; 3],
    ) -> Result<(), EditorError> {
        self.edit_navmesh(form, |geometry| {
            let Some(target) = geometry.vertices.get_mut(vertex) else {
                return Err("vertex index is out of range");
            };
            *target = position;
            Ok(())
        })
    }

    /// Regenerate a NAVM from changed cell geometry while retaining locked
    /// triangles and the geometry that owns authored links.
    ///
    /// For exterior NAVMs the cell coordinates come from the record. Interior
    /// NAVMs use the complete supplied scene. The returned edit is undoable.
    ///
    /// # Errors
    /// Returns an error if the form is not an editable NAVM, generation fails,
    /// a locked triangle index is invalid, or the merged NVNM is invalid.
    pub fn regenerate_navmesh(
        &mut self,
        form: GlobalFormId,
        scene: &NavmeshScene,
        agent: &Agent,
        locked_triangles: &[usize],
    ) -> Result<(), EditorError> {
        let index = self
            .active_record_index(form)
            .ok_or(EditorError::UnsupportedEdit {
                form,
                reason: "navmesh regeneration requires a NAVM record in the active plugin",
            })?;
        let record = &self.active_plugin.records[index];
        if record.tag != Tag(*b"NAVM") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "form is not a NAVM record",
            });
        }
        let view = NavMesh::from_record(record).map_err(|_| EditorError::InvalidNavmesh {
            form,
            reason: "NVNM geometry could not be parsed",
        })?;
        let Some(original) = view.geometry else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        let generated = match original.parent {
            Parent::Grid { x, y, .. } => soe_navmesh::generate_cell(scene, agent, x, y),
            Parent::Cell(_) => soe_navmesh::generate(scene, agent),
        }
        .and_then(|mesh| mesh.to_geometry(original.parent, original.grid.divisor))
        .map_err(|error| EditorError::NavmeshGeneration {
            form,
            message: error.to_string(),
        })?;
        let merged = merge_regenerated_geometry(&original, generated, locked_triangles)
            .map_err(|reason| EditorError::InvalidNavmesh { form, reason })?;

        let mut updated = record.clone();
        let mut nvnm = updated
            .subrecords
            .iter_mut()
            .filter(|subrecord| subrecord.tag == Tag(*b"NVNM"));
        let Some(subrecord) = nvnm.next() else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        if nvnm.next().is_some() {
            return Err(EditorError::RepeatedSubrecord {
                form,
                tag: Tag(*b"NVNM"),
            });
        }
        subrecord.data = merged.to_bytes();
        self.undo.push(self.active_plugin.clone());
        self.active_plugin.records[index] = updated;
        self.rebuild_load_order()?;
        Ok(())
    }

    fn edit_navmesh(
        &mut self,
        form: GlobalFormId,
        edit: impl FnOnce(&mut Geometry) -> Result<(), &'static str>,
    ) -> Result<(), EditorError> {
        let Some(index) = self.active_record_index(form) else {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "navmesh editing requires a NAVM record in the active plugin",
            });
        };
        let record = &self.active_plugin.records[index];
        if record.tag != Tag(*b"NAVM") {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "form is not a NAVM record",
            });
        }
        let view = NavMesh::from_record(record).map_err(|_| EditorError::InvalidNavmesh {
            form,
            reason: "NVNM geometry could not be parsed",
        })?;
        let Some(mut geometry) = view.geometry else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        edit(&mut geometry).map_err(|reason| EditorError::InvalidNavmesh { form, reason })?;
        normalize_geometry(&mut geometry)
            .map_err(|reason| EditorError::InvalidNavmesh { form, reason })?;

        let mut updated = record.clone();
        let mut nvnm = updated
            .subrecords
            .iter_mut()
            .filter(|subrecord| subrecord.tag == Tag(*b"NVNM"));
        let Some(subrecord) = nvnm.next() else {
            return Err(EditorError::InvalidNavmesh {
                form,
                reason: "NAVM has no NVNM geometry",
            });
        };
        if nvnm.next().is_some() {
            return Err(EditorError::RepeatedSubrecord {
                form,
                tag: Tag(*b"NVNM"),
            });
        }
        subrecord.data = geometry.to_bytes();

        self.undo.push(self.active_plugin.clone());
        self.active_plugin.records[index] = updated;
        self.rebuild_load_order()?;
        Ok(())
    }

    /// Write the active plugin to disk and parse the written bytes again.
    ///
    /// # Errors
    /// Returns an error if the destination cannot be written or the produced
    /// plugin bytes cannot be parsed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<Plugin, EditorError> {
        let path = path.as_ref();
        let bytes = self.active_plugin.to_bytes();
        fs::write(path, &bytes).map_err(|source| EditorError::Save {
            path: path.to_path_buf(),
            source,
        })?;
        Plugin::parse(&bytes).map_err(|error| EditorError::Plugin {
            name: path.display().to_string(),
            message: error.to_string(),
        })
    }

    /// Regenerate a NAVM from a changed scene and save the active plugin.
    ///
    /// Exterior scenes should include neighboring geometry around the cell
    /// border so Recast can use it as padding during erosion.
    ///
    /// # Errors
    /// Returns an error if regeneration or plugin saving fails.
    pub fn save_with_navmesh_regeneration(
        &mut self,
        path: impl AsRef<Path>,
        form: GlobalFormId,
        scene: &NavmeshScene,
        agent: &Agent,
        locked_triangles: &[usize],
    ) -> Result<Plugin, EditorError> {
        self.regenerate_navmesh(form, scene, agent, locked_triangles)?;
        self.save(path)
    }

    fn active_record(&self, form: GlobalFormId) -> Option<&Record> {
        let index = self.active_record_index(form)?;
        self.active_plugin.records.get(index)
    }

    fn active_record_index(&self, form: GlobalFormId) -> Option<usize> {
        self.active_plugin
            .records
            .iter()
            .position(|record| self.load_order.resolve(self.active_index, record.form_id) == form)
    }

    fn local_form_id(&self, form: GlobalFormId) -> Result<u32, EditorError> {
        if usize::from(form.file) >= self.load_order.files().len() {
            return Err(EditorError::UnsupportedEdit {
                form,
                reason: "the form owner is outside this load order",
            });
        }
        if form.file as usize == self.active_index {
            return Ok(((self.active_plugin.header.masters.len() as u32) << 24) | form.object);
        }
        let master = self
            .active_plugin
            .header
            .masters
            .iter()
            .position(|name| {
                self.load_order
                    .file(form.file as usize)
                    .name
                    .eq_ignore_ascii_case(name)
            })
            .ok_or(EditorError::UnsupportedEdit {
                form,
                reason: "the active plugin does not list the owning plugin as a master",
            })?;
        Ok(((master as u32) << 24) | form.object)
    }

    fn remap_dialogue_topic_links(
        &self,
        record: &mut Record,
        source_file: usize,
        form: GlobalFormId,
    ) -> Result<(), EditorError> {
        for tag in [Tag(*b"BNAM"), Tag(*b"QNAM")] {
            let Some(subrecord) = record.subrecords.iter_mut().find(|sub| sub.tag == tag) else {
                continue;
            };
            if subrecord.data.len() != 4 {
                return Err(EditorError::UnsupportedEdit {
                    form,
                    reason: "dialogue topic link has an invalid size",
                });
            }
            let bytes = &subrecord.data;
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if raw == 0 {
                continue;
            }
            let target = self.load_order.resolve(source_file, raw);
            subrecord.data[..4].copy_from_slice(&self.local_form_id(target)?.to_le_bytes());
        }
        Ok(())
    }

    fn push_dialogue_info(
        &mut self,
        mut record: Record,
        source_file: usize,
    ) -> Result<(), EditorError> {
        let source_group = record.placement.group.ok_or(EditorError::UnsupportedEdit {
            form: self.load_order.resolve(self.active_index, record.form_id),
            reason: "INFO override has no owning dialogue topic group",
        })?;
        let GroupKind::TopicChildren(source_topic) = source_group else {
            return Err(EditorError::UnsupportedEdit {
                form: self.load_order.resolve(self.active_index, record.form_id),
                reason: "INFO override has no owning dialogue topic group",
            });
        };
        let topic = self.load_order.resolve(source_file, source_topic);
        let local_topic = self.local_form_id(topic)?;
        let top_kind = GroupKind::Top(Tag(*b"DIAL"));
        if self.active_plugin.find_group_mut(top_kind).is_none() {
            self.active_plugin.groups.push(Group::new(top_kind));
        }
        let child = GroupKind::TopicChildren(local_topic);
        let index = self.active_plugin.push_record(record.clone());
        let top =
            self.active_plugin
                .find_group_mut(top_kind)
                .ok_or(EditorError::UnsupportedEdit {
                    form: self.load_order.resolve(source_file, record.form_id),
                    reason: "cannot place INFO override in the dialogue group",
                })?;
        if let Some(Entry::Group(group)) = top
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, Entry::Group(group) if group.kind == child))
        {
            group.entries.push(Entry::Record(index));
        } else {
            let mut group = Group::new(child);
            group.entries.push(Entry::Record(index));
            top.entries.push(Entry::Group(group));
        }
        record.placement.group = Some(child);
        self.active_plugin.records[index].placement = record.placement;
        Ok(())
    }

    fn rebuild_load_order(&mut self) -> Result<(), EditorError> {
        let mut rebuilt = LoadOrder::new();
        for (index, file) in self.load_order.files().iter().enumerate() {
            let plugin = if index == self.active_index {
                self.active_plugin.clone()
            } else {
                file.plugin.clone()
            };
            rebuilt
                .add(&file.name, plugin)
                .map_err(|error| EditorError::Plugin {
                    name: file.name.clone(),
                    message: error.to_string(),
                })?;
        }
        self.load_order = rebuilt;
        Ok(())
    }

    fn sync_record_count(&mut self) {
        let count = self.active_plugin.records.len() as u32;
        self.active_plugin.header.record_count = count;
        if let Some(hedr) = self
            .active_plugin
            .header_record
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.tag == Tag(*b"HEDR"))
        {
            hedr.data[4..8].copy_from_slice(&count.to_le_bytes());
        }
    }
}

fn contains_ascii_case_insensitive(value: &[u8], query: &str) -> bool {
    let value = value.strip_suffix(&[0]).unwrap_or(value);
    value
        .windows(query.len())
        .any(|window| window.eq_ignore_ascii_case(query.as_bytes()))
}

fn merge_regenerated_geometry(
    original: &Geometry,
    mut generated: Geometry,
    requested_locks: &[usize],
) -> Result<Geometry, &'static str> {
    if original.parent != generated.parent {
        return Err("generated parent does not match the edited NAVM");
    }
    let mut retained = BTreeSet::new();
    for &triangle in requested_locks {
        if triangle >= original.triangles.len() {
            return Err("locked triangle index is out of range");
        }
        retained.insert(triangle);
    }
    for (triangle_index, triangle) in original.triangles.iter().enumerate() {
        if (0..3).any(|edge| {
            triangle.flags & (1 << edge) != 0
                && usize::try_from(triangle.edges[edge])
                    .is_ok_and(|link| link < original.edge_links.len())
        }) {
            retained.insert(triangle_index);
        }
    }
    for link in &original.door_links {
        let index =
            usize::try_from(link.triangle).map_err(|_| "door link triangle index is negative")?;
        if index >= original.triangles.len() {
            return Err("door link triangle index is out of range");
        }
        retained.insert(index);
    }

    let mut old_to_new = vec![None; original.triangles.len()];
    let mut old_vertex_to_new = Vec::with_capacity(original.vertices.len());
    for &position in &original.vertices {
        let index = if let Some(index) = generated
            .vertices
            .iter()
            .position(|candidate| *candidate == position)
        {
            index
        } else {
            let index = generated.vertices.len();
            generated.vertices.push(position);
            index
        };
        old_vertex_to_new.push(u16::try_from(index).map_err(|_| "vertex index overflow")?);
    }
    for old_index in retained {
        let triangle = original.triangles[old_index];
        let vertices = triangle
            .vertices
            .map(|vertex| old_vertex_to_new[usize::from(vertex)]);
        let next =
            i16::try_from(generated.triangles.len()).map_err(|_| "triangle index overflow")?;
        generated.triangles.push(Triangle {
            vertices,
            edges: triangle.edges,
            flags: triangle.flags,
            cover_flags: triangle.cover_flags,
        });
        old_to_new[old_index] = Some(usize::try_from(next).map_err(|_| "triangle index overflow")?);
    }
    generated.edge_links.clone_from(&original.edge_links);
    generated.door_links = original
        .door_links
        .iter()
        .filter_map(|link| remap_door_link(*link, &old_to_new))
        .collect();
    generated.cover_triangles = original
        .cover_triangles
        .iter()
        .filter_map(|triangle| {
            usize::try_from(*triangle)
                .ok()
                .and_then(|index| old_to_new.get(index).copied().flatten())
                .and_then(|index| i16::try_from(index).ok())
        })
        .collect();

    normalize_geometry(&mut generated)?;
    for (old_index, new_index) in old_to_new.iter().enumerate() {
        let Some(new_index) = new_index else { continue };
        let old = original.triangles[old_index];
        let current = &mut generated.triangles[*new_index];
        for edge in 0..3 {
            let edge_bit = 1 << edge;
            if old.flags & edge_bit != 0 {
                current.flags |= edge_bit;
                current.edges[edge] = old.edges[edge];
            }
        }
    }
    validate_metadata(&generated)?;
    Ok(generated)
}

fn remap_door_link(link: DoorLink, mapping: &[Option<usize>]) -> Option<DoorLink> {
    let old_index = usize::try_from(link.triangle).ok()?;
    let new_index = mapping.get(old_index).copied().flatten()?;
    Some(DoorLink {
        triangle: i16::try_from(new_index).ok()?,
        crc: link.crc,
        door: link.door,
    })
}

const EDGE_LINK_BITS: [u16; 3] = [1, 2, 4];

fn normalize_geometry(geometry: &mut Geometry) -> Result<(), &'static str> {
    if geometry.vertices.is_empty() || geometry.vertices.len() > u16::MAX as usize + 1 {
        return Err("vertex count exceeds NVNM unsigned index capacity");
    }
    if geometry.triangles.len() > i16::MAX as usize + 1 {
        return Err("triangle count exceeds NVNM signed index capacity");
    }
    if geometry
        .vertices
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err("vertex coordinates must be finite");
    }
    for triangle in &geometry.triangles {
        let [a, b, c] = triangle.vertices.map(usize::from);
        if a >= geometry.vertices.len()
            || b >= geometry.vertices.len()
            || c >= geometry.vertices.len()
        {
            return Err("triangle references a missing vertex");
        }
        let [a, b, c] = [
            geometry.vertices[a],
            geometry.vertices[b],
            geometry.vertices[c],
        ];
        let cross = [
            (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
            (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
        ];
        if cross
            .iter()
            .all(|component| component.abs() <= f32::EPSILON)
        {
            return Err("triangle vertices must not be collinear");
        }
    }

    let mut edge_owners = std::collections::BTreeMap::<(u16, u16), Vec<(usize, usize)>>::new();
    for (triangle_index, triangle) in geometry.triangles.iter().enumerate() {
        for edge in 0..3 {
            let a = triangle.vertices[edge];
            let b = triangle.vertices[(edge + 1) % 3];
            edge_owners
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((triangle_index, edge));
        }
    }
    for owners in edge_owners.values() {
        if owners.len() > 2 {
            return Err("more than two triangles share an edge");
        }
    }
    for (owner_index, triangle) in geometry.triangles.iter_mut().enumerate() {
        for (edge, &link_bit) in EDGE_LINK_BITS.iter().enumerate() {
            if triangle.flags & link_bit == 0 {
                triangle.edges[edge] = -1;
            }
            let neighbor = edge_owners
                .get(&{
                    let a = triangle.vertices[edge];
                    let b = triangle.vertices[(edge + 1) % 3];
                    (a.min(b), a.max(b))
                })
                .and_then(|owners| {
                    owners
                        .iter()
                        .find(|(other_index, _)| *other_index != owner_index)
                });
            if let Some((other_index, _)) = neighbor {
                triangle.edges[edge] =
                    i16::try_from(*other_index).map_err(|_| "triangle index overflow")?;
                triangle.flags &= !link_bit;
            }
        }
    }
    rebuild_grid(geometry)?;
    validate_metadata(geometry)?;
    Ok(())
}

fn rebuild_grid(geometry: &mut Geometry) -> Result<(), &'static str> {
    let divisor = geometry.grid.divisor;
    let cell_count = usize::try_from(divisor)
        .ok()
        .and_then(|side| side.checked_mul(side))
        .filter(|count| *count > 0 && *count == geometry.grid.cells.len())
        .ok_or("navmesh grid divisor does not match its cells")?;
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for vertex in &geometry.vertices {
        for axis in 0..3 {
            min[axis] = min[axis].min(vertex[axis]);
            max[axis] = max[axis].max(vertex[axis]);
        }
    }
    let width = (max[0] - min[0]).max(1.0);
    let height = (max[1] - min[1]).max(1.0);
    let side = divisor as usize;
    let mut cells = vec![Vec::new(); cell_count];
    for (triangle_index, triangle) in geometry.triangles.iter().enumerate() {
        let points = triangle
            .vertices
            .map(|index| geometry.vertices[usize::from(index)]);
        let x0 = points
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let x1 = points
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let y0 = points
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let y1 = points
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let cell_x = |value: f32| {
            (((value - min[0]) / width * divisor as f32).floor() as usize).min(side - 1)
        };
        let cell_y = |value: f32| {
            (((value - min[1]) / height * divisor as f32).floor() as usize).min(side - 1)
        };
        for y in cell_y(y0)..=cell_y(y1) {
            for x in cell_x(x0)..=cell_x(x1) {
                cells[y * side + x]
                    .push(i16::try_from(triangle_index).map_err(|_| "triangle index overflow")?);
            }
        }
    }
    geometry.grid.size = [width / divisor as f32, height / divisor as f32];
    geometry.grid.min = min;
    geometry.grid.max = max;
    geometry.grid.cells = cells;
    Ok(())
}

fn validate_metadata(geometry: &Geometry) -> Result<(), &'static str> {
    let count = geometry.triangles.len();
    for triangle in &geometry.triangles {
        for (edge, &link_bit) in EDGE_LINK_BITS.iter().enumerate() {
            let raw = triangle.edges[edge];
            if triangle.flags & link_bit != 0 {
                if usize::try_from(raw)
                    .ok()
                    .is_none_or(|index| index >= geometry.edge_links.len())
                {
                    return Err("triangle edge references a missing edge link");
                }
            } else if raw < -1 || usize::try_from(raw).is_ok_and(|index| index >= count) {
                return Err("triangle edge references a missing triangle");
            }
        }
    }
    if geometry
        .door_links
        .iter()
        .any(|link| usize::try_from(link.triangle).map_or(true, |index| index >= count))
        || geometry
            .cover_triangles
            .iter()
            .any(|index| usize::try_from(*index).map_or(true, |index| index >= count))
        || geometry
            .grid
            .cells
            .iter()
            .flatten()
            .any(|index| usize::try_from(*index).map_or(true, |index| index >= count))
    {
        return Err("triangle metadata references a missing triangle");
    }
    Ok(())
}

fn reject_localized_quest_text(form: GlobalFormId, record: &Record) -> Result<(), EditorError> {
    if record.flags & soe_esm::plugin::flags::LOCALIZED != 0 {
        return Err(EditorError::LocalizedQuestText(form));
    }
    Ok(())
}

fn quest_alias_range(
    form: GlobalFormId,
    record: &Record,
    alias_occurrence: usize,
) -> Result<(usize, usize), EditorError> {
    let alias_tag = Tag(*b"ALST");
    let aliases = record
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.tag == alias_tag).then_some(index))
        .collect::<Vec<_>>();
    let start = *aliases
        .get(alias_occurrence)
        .ok_or(EditorError::InvalidQuestField {
            form,
            tag: alias_tag,
            reason: "alias occurrence does not exist",
        })?;
    let boundary = aliases
        .get(alias_occurrence + 1)
        .copied()
        .unwrap_or(record.subrecords.len());
    let terminators = (start + 1..boundary)
        .filter(|index| record.subrecords[*index].tag == Tag(*b"ALED"))
        .collect::<Vec<_>>();
    let [terminator] = terminators.as_slice() else {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: Tag(*b"ALED"),
            reason: "alias must have exactly one end marker",
        });
    };
    let end = *terminator + 1;
    if record.subrecords[start].data.len() != 4 {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: alias_tag,
            reason: "alias id must contain four bytes",
        });
    }
    let body = &record.subrecords[start + 1..end - 1];
    let alias_name_count = body
        .iter()
        .filter(|subrecord| subrecord.tag == Tag(*b"ALID"))
        .count();
    let flags = body
        .iter()
        .filter(|subrecord| subrecord.tag == Tag(*b"FNAM"))
        .collect::<Vec<_>>();
    if alias_name_count != 1 || flags.len() != 1 || flags[0].data.len() != 4 {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: alias_tag,
            reason: "reference alias needs one name and one four-byte flags field",
        });
    }
    if !record.subrecords[*terminator].data.is_empty() {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: Tag(*b"ALED"),
            reason: "alias end marker must be empty",
        });
    }
    Ok((start, end))
}

fn is_condition_subrecord(tag: Tag) -> bool {
    matches!(tag.as_bytes(), b"CTDA" | b"CIS1" | b"CIS2")
}

fn is_alias_fill_subrecord(tag: Tag) -> bool {
    matches!(
        tag.as_bytes(),
        b"ALFI"
            | b"ALFR"
            | b"ALUA"
            | b"ALFA"
            | b"ALRT"
            | b"ALEQ"
            | b"ALEA"
            | b"ALCO"
            | b"ALCA"
            | b"ALCL"
            | b"ALNA"
            | b"ALNT"
            | b"ALFE"
            | b"ALFD"
    )
}

fn validate_alias_conditions(
    form: GlobalFormId,
    record: &Record,
    conditions: &[Subrecord],
) -> Result<(), EditorError> {
    let mut condition_record = record.clone();
    condition_record.subrecords = conditions.to_vec();
    let mut parsed = Vec::new();
    for subrecord in conditions {
        if !is_condition_subrecord(subrecord.tag)
            || soe_esm::records::gameplay::Condition::push(
                &condition_record,
                subrecord,
                &mut parsed,
            )
            .is_err()
        {
            return Err(EditorError::InvalidQuestConditions {
                form,
                reason: "expected well-formed CTDA records and optional CIS1 or CIS2 strings",
            });
        }
    }
    Ok(())
}

fn nul_terminated(text: &str) -> Vec<u8> {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    data
}

fn quest_stage_log_text_index(
    form: GlobalFormId,
    record: &Record,
    stage_occurrence: usize,
    entry_occurrence: usize,
) -> Result<usize, EditorError> {
    let stage_tag = Tag(*b"INDX");
    let qobj = Tag(*b"QOBJ");
    let qsdt = Tag(*b"QSDT");
    let cnam = Tag(*b"CNAM");
    let stages = record
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.tag == stage_tag).then_some(index))
        .collect::<Vec<_>>();
    let start = *stages
        .get(stage_occurrence)
        .ok_or(EditorError::InvalidQuestField {
            form,
            tag: stage_tag,
            reason: "stage occurrence does not exist",
        })?;
    if record.subrecords[start].data.len() != 4 {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: stage_tag,
            reason: "stage index must contain four bytes",
        });
    }
    let end = stages
        .get(stage_occurrence + 1)
        .copied()
        .unwrap_or(record.subrecords.len());
    let end = (start + 1..end)
        .find(|index| {
            let tag = record.subrecords[*index].tag;
            tag == qobj || tag == Tag(*b"ANAM") || tag == Tag(*b"ALST") || tag == Tag(*b"ALLS")
        })
        .unwrap_or(end);
    let entries = (start + 1..end)
        .filter(|index| record.subrecords[*index].tag == qsdt)
        .collect::<Vec<_>>();
    let entry_start = *entries
        .get(entry_occurrence)
        .ok_or(EditorError::InvalidQuestField {
            form,
            tag: qsdt,
            reason: "stage log entry occurrence does not exist",
        })?;
    if record.subrecords[entry_start].data.len() != 1 {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: qsdt,
            reason: "stage log flags must contain one byte",
        });
    }
    let entry_end = entries.get(entry_occurrence + 1).copied().unwrap_or(end);
    let text_fields = (entry_start + 1..entry_end)
        .filter(|index| record.subrecords[*index].tag == cnam)
        .collect::<Vec<_>>();
    match text_fields.as_slice() {
        [index] => Ok(*index),
        [] => Err(EditorError::InvalidQuestField {
            form,
            tag: cnam,
            reason: "stage log entry has no text field",
        }),
        _ => Err(EditorError::InvalidQuestField {
            form,
            tag: cnam,
            reason: "stage log entry has repeated text fields",
        }),
    }
}

fn quest_objective_text_index(
    form: GlobalFormId,
    record: &Record,
    objective_occurrence: usize,
) -> Result<usize, EditorError> {
    let qobj = Tag(*b"QOBJ");
    let nnam = Tag(*b"NNAM");
    let objectives = record
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.tag == qobj).then_some(index))
        .collect::<Vec<_>>();
    let start = *objectives
        .get(objective_occurrence)
        .ok_or(EditorError::InvalidQuestField {
            form,
            tag: qobj,
            reason: "objective occurrence does not exist",
        })?;
    if record.subrecords[start].data.len() != 2 {
        return Err(EditorError::InvalidQuestField {
            form,
            tag: qobj,
            reason: "objective index must contain two bytes",
        });
    }
    let end = objectives
        .get(objective_occurrence + 1)
        .copied()
        .unwrap_or(record.subrecords.len());
    let end = (start + 1..end)
        .find(|index| {
            matches!(
                record.subrecords[*index].tag,
                tag if tag == Tag(*b"ANAM") || tag == Tag(*b"ALST") || tag == Tag(*b"ALLS")
            )
        })
        .unwrap_or(end);
    let text_fields = (start + 1..end)
        .filter(|index| record.subrecords[*index].tag == nnam)
        .collect::<Vec<_>>();
    match text_fields.as_slice() {
        [index] => Ok(*index),
        [] => Err(EditorError::InvalidQuestField {
            form,
            tag: nnam,
            reason: "objective has no display text",
        }),
        _ => Err(EditorError::InvalidQuestField {
            form,
            tag: nnam,
            reason: "objective has repeated display text",
        }),
    }
}

fn zstring_bytes(value: &str) -> Vec<u8> {
    let mut bytes = value.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn scene_phase_ranges(record: &Record) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, subrecord) in record.subrecords.iter().enumerate() {
        if subrecord.tag != Tag(*b"HNAM") || !subrecord.data.is_empty() {
            continue;
        }
        if let Some(phase_start) = start.take() {
            ranges.push((phase_start, index));
        } else {
            start = Some(index + 1);
        }
    }
    if let Some(phase_start) = start {
        ranges.push((phase_start, record.subrecords.len()));
    }
    ranges
}

fn scene_action_ranges(record: &Record) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, subrecord) in record.subrecords.iter().enumerate() {
        if subrecord.tag != Tag(*b"ANAM") {
            continue;
        }
        if subrecord.data.is_empty() {
            if let Some(action_start) = start.take() {
                ranges.push((action_start, index));
            }
        } else if start.is_none() {
            start = Some(index + 1);
        }
    }
    if let Some(action_start) = start {
        ranges.push((action_start, record.subrecords.len()));
    }
    ranges
}

fn set_scene_field(
    record: &mut Record,
    range: (usize, usize),
    tag: Tag,
    data: Vec<u8>,
    form: GlobalFormId,
) -> Result<(), EditorError> {
    let matching = record.subrecords[range.0..range.1]
        .iter()
        .enumerate()
        .filter(|(_, subrecord)| subrecord.tag == tag)
        .map(|(offset, _)| range.0 + offset)
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [index] => record.subrecords[*index].data = data,
        [] => record.subrecords.insert(range.1, Subrecord::new(tag, data)),
        _ => {
            return Err(EditorError::InvalidScene {
                form,
                reason: "field repeats inside its scene block",
            });
        }
    }
    Ok(())
}

fn relocate_scene_form_id(
    document: &EditorDocument,
    subrecord: &mut Subrecord,
    source_file: usize,
    expected_tag: [u8; 4],
    form: GlobalFormId,
) -> Result<(), EditorError> {
    if subrecord.data.len() != 4 {
        return Err(EditorError::InvalidScene {
            form,
            reason: "linked FormID has an invalid size",
        });
    }
    relocate_scene_id(
        document,
        &mut subrecord.data,
        source_file,
        expected_tag,
        form,
    )
}

fn relocate_scene_id(
    document: &EditorDocument,
    bytes: &mut [u8],
    source_file: usize,
    expected_tag: [u8; 4],
    form: GlobalFormId,
) -> Result<(), EditorError> {
    if bytes.len() != 4 {
        return Err(EditorError::InvalidScene {
            form,
            reason: "linked FormID has an invalid size",
        });
    }
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if raw == 0 && expected_tag == *b"DIAL" {
        return Ok(());
    }
    document.validate_scene_link(source_file, raw, expected_tag, false, form)?;
    let resolved = document.load_order.resolve(source_file, raw);
    let relocated = document
        .local_form_id(resolved)
        .map_err(|_| EditorError::InvalidScene {
            form,
            reason: "active plugin does not list a linked form owner as a master",
        })?;
    bytes.copy_from_slice(&relocated.to_le_bytes());
    Ok(())
}

fn file_name(path: &Path) -> Result<String, EditorError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| EditorError::Plugin {
            name: path.display().to_string(),
            message: "plugin path has no UTF-8 file name".to_string(),
        })
}

fn encode_vmad(object: &pex::Object) -> Result<Vec<u8>, EditorError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    write_vmad_string(&mut bytes, &object.name)?;
    bytes.push(0);
    let property_count = u16::try_from(object.properties.len())
        .map_err(|_| EditorError::Papyrus("too many script properties".into()))?;
    bytes.extend_from_slice(&property_count.to_le_bytes());
    for property in &object.properties {
        write_vmad_string(&mut bytes, &property.name)?;
        let property_type = if property.type_name.eq_ignore_ascii_case("string") {
            2
        } else if property.type_name.eq_ignore_ascii_case("int") {
            3
        } else if property.type_name.eq_ignore_ascii_case("float") {
            4
        } else if property.type_name.eq_ignore_ascii_case("bool") {
            5
        } else {
            1
        };
        bytes.push(property_type);
        bytes.push(1);
        match property_type {
            1 => bytes.extend_from_slice(&[0; 8]),
            2 => write_vmad_string(&mut bytes, "")?,
            3 => bytes.extend_from_slice(&0i32.to_le_bytes()),
            4 => bytes.extend_from_slice(&0f32.to_le_bytes()),
            5 => bytes.push(0),
            _ => return Err(EditorError::Papyrus("unsupported property type".into())),
        }
    }
    Ok(bytes)
}

fn write_vmad_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), EditorError> {
    let utf16: Vec<u16> = value.encode_utf16().collect();
    let length = u16::try_from(utf16.len())
        .map_err(|_| EditorError::Papyrus("VMAD string exceeds 65535 UTF-16 units".into()))?;
    bytes.extend_from_slice(&length.to_le_bytes());
    for unit in utf16 {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(())
}

fn quest_master_order_is_aligned(
    load_order: &LoadOrder,
    active_index: usize,
    source_index: usize,
) -> bool {
    let active_masters = &load_order.file(active_index).plugin.header.masters;
    let source_masters = &load_order.file(source_index).plugin.header.masters;
    source_masters.len() == source_index
        && active_masters.len() > source_index
        && (0..source_index).all(|index| {
            source_masters[index].eq_ignore_ascii_case(&load_order.file(index).name)
                && active_masters[index].eq_ignore_ascii_case(&load_order.file(index).name)
        })
        && active_masters[source_index].eq_ignore_ascii_case(&load_order.file(source_index).name)
}

fn is_top_level(plugin: &Plugin, record_index: usize) -> bool {
    plugin.groups.iter().any(|group| {
        matches!(group.kind, GroupKind::Top(_))
            && group
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Record(index) if *index == record_index))
    })
}
