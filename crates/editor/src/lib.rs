// SPDX-License-Identifier: GPL-3.0-or-later

//! A load-order-backed plugin editing document with undo and plugin output.

use std::fs;
use std::path::{Path, PathBuf};

use soe_esm::{Entry, GlobalFormId, GroupKind, LoadOrder, Plugin, Record, Subrecord, Tag};
use soe_papyrus::{pex, psc};
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

fn is_top_level(plugin: &Plugin, record_index: usize) -> bool {
    plugin.groups.iter().any(|group| {
        matches!(group.kind, GroupKind::Top(_))
            && group
                .entries
                .iter()
                .any(|entry| matches!(entry, Entry::Record(index) if *index == record_index))
    })
}
