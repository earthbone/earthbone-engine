// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

use soe_esm::{Entry, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Subrecord, Tag};
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
    /// The requested response is absent from an INFO record.
    #[error("form {form} has no response at index {response}")]
    MissingResponse {
        /// INFO form being edited.
        form: GlobalFormId,
        /// Zero-based response index.
        response: usize,
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
                if source.tag != Tag(*b"STAT") && !dialogue_topic && !dialogue_info {
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
