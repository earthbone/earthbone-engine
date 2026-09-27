// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

use soe_esm::records::gameplay::{Scene, SceneActionData};
use soe_esm::{Entry, GlobalFormId, GroupKind, LoadOrder, Plugin, Record, Subrecord, Tag};
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
    /// A scene edit would create an invalid record or link.
    #[error("cannot edit scene {form}: {reason}")]
    InvalidScene {
        /// Scene being edited.
        form: GlobalFormId,
        /// Reason the scene edit was refused.
        reason: &'static str,
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

    /// Replace or append one subrecord, creating an active-plugin override.
    ///
    /// This milestone accepts top-level records only. Existing linked fields
    /// are copied as authored, so forms requiring FormID remapping are
    /// rejected until the ESM renumbering work is available.
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
                if !is_top_level(
                    &self.load_order.file(record_ref.file).plugin,
                    record_ref.record,
                ) {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "nested record placement needs FMT-15 and FMT-26 FormID remapping",
                    });
                }
                if source.tag != Tag(*b"STAT") {
                    return Err(EditorError::UnsupportedEdit {
                        form,
                        reason: "record reference relocation is not available before FMT-15",
                    });
                }
                let mut record = source.clone();
                record.form_id = self.local_form_id(form)?;
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
            self.active_plugin.push_top(record);
        } else if let Some(index) = self.active_record_index(form) {
            self.active_plugin.records[index] = record;
        }
        self.sync_record_count();
        self.rebuild_load_order()?;
        Ok(())
    }

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
        let scene = Scene::from_record(&record).map_err(|_| EditorError::InvalidScene {
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
        let scene = Scene::from_record(&record).map_err(|_| EditorError::InvalidScene {
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
        let scene = Scene::from_record(record).map_err(|_| EditorError::InvalidScene {
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

fn is_top_level(plugin: &Plugin, record_index: usize) -> bool {
    plugin.groups.iter().any(|group| {
        matches!(group.kind, GroupKind::Top(_))
            && group
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Record(index) if *index == record_index))
    })
}
