// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

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

    /// Apply one circular texture brush dab to a LAND quadrant in the active plugin.
    ///
    /// The center and radius use cell-local world units. A missing texture
    /// layer is created at the first unused quadrant layer index. One dab is
    /// stored as one undo step.
    ///
    /// # Errors
    /// Returns an error if the form is missing, is not an active-plugin LAND
    /// record, or its quadrant layers cannot be edited.
    pub fn brush_land_texture(
        &mut self,
        form: GlobalFormId,
        texture: GlobalFormId,
        quadrant: u8,
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
                reason: "texture brush requires an active-plugin LAND record",
            });
        }
        let texture = self.local_form_id(texture)?;
        let mut record = self.active_plugin.records[index].clone();
        soe_esm::records::land::apply_texture_brush(
            &mut record,
            texture,
            quadrant,
            center,
            radius,
            strength,
        )
        .map_err(|error| EditorError::Plugin {
            name: self.active_name.clone(),
            message: error.to_string(),
        })?;

        if record != self.active_plugin.records[index] {
            self.undo.push(self.active_plugin.clone());
            self.active_plugin.records[index] = record;
            self.rebuild_load_order()?;
        }
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
