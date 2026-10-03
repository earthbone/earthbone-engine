// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot editor bridge for the load-order-backed forms dock.
//!
//! The dock sends a signature and text query to `soe-editor`, which returns
//! winning FormIDs. A Godot Resource is created only when the dock selects an
//! ID, following Godot's EditorPlugin and EditorInspectorPlugin APIs.

// CONTEXT: gdext's class(init) expansion emits explicit field names.
#![allow(clippy::redundant_field_names)]

use std::path::PathBuf;

use godot::classes::RefCounted;
use godot::prelude::*;
use soe_editor::EditorDocument;
use soe_esm::records::refr::Refr;
use soe_esm::{GlobalFormId, Tag};

use crate::godot_physics::ground_collision_position;
use crate::units::SkyrimPos;

/// A selected winning form displayed by the Godot inspector.
#[derive(GodotClass)]
#[class(init, base = Resource)]
pub struct SoeFormResource {
    base: Base<Resource>,
    /// Stable load-order file index for this form.
    #[var]
    pub file_index: i32,
    /// Object portion of the resolved FormID.
    #[var]
    pub object_id: i64,
    /// Four-character record signature.
    #[var]
    pub signature: GString,
    /// Editor ID of the winning record, when it has one.
    #[var]
    pub editor_id: GString,
    /// File that supplies the winning record.
    #[var]
    pub winning_plugin: GString,
}

/// Load-order-backed form queries for the editor plugin.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct SoeFormTable {
    base: Base<RefCounted>,
    document: Option<EditorDocument>,
    last_error: GString,
    resources_created: i64,
}

#[godot_api]
impl SoeFormTable {
    /// Query the first host collision around a Skyrim-space point.
    #[func]
    pub fn query_ground_collision(
        &self,
        space: Rid,
        x: f64,
        y: f64,
        z: f64,
        above_units: f64,
        below_units: f64,
    ) -> VarDictionary {
        let mut result = VarDictionary::new();
        match ground_collision_position(
            space,
            SkyrimPos::new(x as f32, y as f32, z as f32),
            above_units as f32,
            below_units as f32,
        ) {
            Ok(Some(position)) => {
                result.set("hit", true);
                result.set("position", Vector3::new(position.x, position.y, position.z));
            }
            Ok(None) => result.set("hit", false),
            Err(error) => {
                result.set("hit", false);
                result.set("error", error);
            }
        }
        result
    }

    /// Read the winning REFR position and Euler rotation for an encoded FormID.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn reference_transform(&self, encoded_id: GString) -> VarDictionary {
        let mut result = VarDictionary::new();
        let Some(document) = self.document.as_ref() else {
            return result;
        };
        let Some(id) = parse_form_id(&encoded_id.to_string()) else {
            return result;
        };
        let Some(record) = document.load_order().winner(id) else {
            return result;
        };
        let Ok(reference) = Refr::from_record(record) else {
            return result;
        };
        let Some(transform) = reference.transform else {
            return result;
        };
        result.set(
            "position",
            Vector3::new(
                transform.position[0],
                transform.position[1],
                transform.position[2],
            ),
        );
        result.set(
            "rotation",
            Vector3::new(
                transform.rotation[0],
                transform.rotation[1],
                transform.rotation[2],
            ),
        );
        result
    }

    /// Open the plugin paths in load-order sequence and select the active file.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn open(&mut self, paths: PackedStringArray, active_plugin: GString) -> bool {
        let paths = paths
            .as_slice()
            .iter()
            .map(|path| PathBuf::from(path.to_string()))
            .collect::<Vec<_>>();
        match EditorDocument::open(&paths, &active_plugin.to_string()) {
            Ok(document) => {
                self.document = Some(document);
                self.last_error = GString::new();
                self.resources_created = 0;
                true
            }
            Err(error) => {
                self.document = None;
                self.last_error = GString::from(error.to_string().as_str());
                false
            }
        }
    }

    /// Return a diagnostic from the most recent failed open.
    #[func]
    pub fn last_error(&self) -> GString {
        self.last_error.clone()
    }

    /// Search winning forms and return encoded IDs without creating Resources.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn search(
        &self,
        signature: GString,
        query: GString,
        offset: i64,
        limit: i64,
    ) -> PackedStringArray {
        let mut results = PackedStringArray::new();
        let Some(document) = &self.document else {
            return results;
        };
        let signature = signature.to_string();
        if !signature.is_empty()
            && (signature.len() != 4 || !signature.as_bytes().iter().all(u8::is_ascii))
        {
            return results;
        }
        let tag = (!signature.is_empty()).then(|| Tag(signature_tag(&signature)));
        let offset = usize::try_from(offset.max(0)).unwrap_or_default();
        let limit = usize::try_from(limit.clamp(0, 200)).unwrap_or_default();
        for id in search_form_ids(document, tag, &query.to_string(), offset, limit) {
            results.push(&id);
        }
        results
    }

    /// Build an inspector Resource for one selected form ID.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn selected_form(&mut self, encoded_id: GString) -> Option<Gd<SoeFormResource>> {
        let document = self.document.as_ref()?;
        let id = parse_form_id(&encoded_id.to_string())?;
        let record_ref = document.load_order().winner_ref(id)?;
        let record = document.load_order().record(record_ref);
        let plugin = &document.load_order().file(record_ref.file).name;
        let signature = String::from_utf8_lossy(&record.tag.0).into_owned();
        let editor_id = record.editor_id().unwrap_or_default();
        let winning_plugin = plugin.clone();
        let mut resource = Gd::<SoeFormResource>::from_init_fn(|base| SoeFormResource {
            base,
            file_index: i32::from(id.file),
            object_id: i64::from(id.object),
            signature: GString::from(signature.as_str()),
            editor_id: GString::from(editor_id.as_str()),
            winning_plugin: GString::from(winning_plugin.as_str()),
        });
        let resource_name = format!("{} {}", record.tag, encoded_id);
        resource.set_name(resource_name.as_str());
        self.resources_created += 1;
        Some(resource)
    }

    /// Return each selected form version with its source and winning status.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn override_chain(&self, encoded_id: GString) -> Array<Dictionary<Variant, Variant>> {
        let mut versions = Array::new();
        let Some(document) = &self.document else {
            return versions;
        };
        let Some(id) = parse_form_id(&encoded_id.to_string()) else {
            return versions;
        };
        for version in override_chain_entries(document, id) {
            let mut item = Dictionary::new();
            item.set("plugin_name", version.plugin_name.as_str());
            item.set("signature", version.signature.as_str());
            item.set("editor_id", version.editor_id.as_str());
            item.set("is_winner", version.is_winner);
            versions.push(&item);
        }
        versions
    }

    /// Drop an active-plugin REFR onto the first host collision below it.
    ///
    /// The vertical host ray starts 100 Skyrim units above the reference and
    /// ends 10,000 units below it. The record edit keeps horizontal position
    /// and rotation, and the document edit log makes it undoable.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn drop_selected_reference_to_ground(&mut self, space: Rid, encoded_id: GString) -> bool {
        let Some(document) = self.document.as_mut() else {
            self.last_error = GString::from("no editor document is open");
            return false;
        };
        let Some(id) = parse_form_id(&encoded_id.to_string()) else {
            self.last_error = GString::from("selected form id is invalid");
            return false;
        };
        let Some(record) = document.load_order().winner(id) else {
            self.last_error = GString::from("selected reference has no winning record");
            return false;
        };
        let transform = if record.tag == Tag(*b"REFR") {
            Refr::from_record(record)
                .ok()
                .and_then(|refr| refr.transform)
        } else {
            None
        };
        let Some(transform) = transform else {
            self.last_error = GString::from("selected form has no valid REFR transform");
            return false;
        };
        let position = transform.position;
        let collision = ground_collision_position(
            space,
            SkyrimPos::new(position[0], position[1], position[2]),
            100.0,
            10_000.0,
        );
        let hit = match collision {
            Ok(Some(hit)) => hit,
            Ok(None) => {
                self.last_error = GString::from("no host collision below selected reference");
                return false;
            }
            Err(error) => {
                self.last_error = GString::from(error);
                return false;
            }
        };
        match apply_ground_height(document, id, hit.z) {
            Ok(()) => {
                self.last_error = GString::new();
                true
            }
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                false
            }
        }
    }

    /// Undo the latest editor record edit.
    #[func]
    pub fn undo(&mut self) -> bool {
        let Some(document) = self.document.as_mut() else {
            return false;
        };
        match document.undo() {
            Ok(undone) => undone,
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                false
            }
        }
    }

    /// Count selected-form Resources created since the last successful open.
    #[func]
    pub fn resources_created(&self) -> i64 {
        self.resources_created
    }
}

struct OverrideChainEntry {
    plugin_name: String,
    signature: String,
    editor_id: String,
    is_winner: bool,
}

fn override_chain_entries(document: &EditorDocument, id: GlobalFormId) -> Vec<OverrideChainEntry> {
    document
        .override_chain(id)
        .into_iter()
        .map(|version| OverrideChainEntry {
            plugin_name: version.plugin_name.to_owned(),
            signature: String::from_utf8_lossy(&version.record.tag.0).into_owned(),
            editor_id: version.record.editor_id().unwrap_or_default(),
            is_winner: version.is_winner,
        })
        .collect()
}

fn apply_ground_height(
    document: &mut EditorDocument,
    id: GlobalFormId,
    height: f32,
) -> Result<(), soe_editor::EditorError> {
    let record = document
        .load_order()
        .winner(id)
        .ok_or(soe_editor::EditorError::MissingForm(id))?;
    if record.tag != Tag(*b"REFR") {
        return Err(soe_editor::EditorError::UnsupportedEdit {
            form: id,
            reason: "the selected form is not a placed reference",
        });
    }
    let transform = Refr::from_record(record)
        .ok()
        .and_then(|reference| reference.transform)
        .ok_or(soe_editor::EditorError::UnsupportedEdit {
            form: id,
            reason: "the placed reference has no valid transform DATA",
        })?;
    let drop =
        transform
            .drop_to_ground(height)
            .ok_or(soe_editor::EditorError::InvalidFieldValue {
                form: id,
                field: "REFR.DATA.Z",
                reason: "collision height must be finite",
            })?;
    document.set_reference_transform(id, drop.placed())
}

fn search_form_ids(
    document: &EditorDocument,
    tag: Option<Tag>,
    query: &str,
    offset: usize,
    limit: usize,
) -> Vec<String> {
    document
        .search_forms(tag, query)
        .skip(offset)
        .take(limit)
        .map(|id| format!("{}:{:06X}", id.file, id.object))
        .collect()
}

fn signature_tag(signature: &str) -> [u8; 4] {
    let mut bytes = [0; 4];
    if signature.len() == bytes.len() {
        bytes.copy_from_slice(signature.as_bytes());
    } else {
        bytes = [0; 4];
    }
    bytes
}

fn parse_form_id(value: &str) -> Option<GlobalFormId> {
    let (file, object) = value.split_once(':')?;
    Some(GlobalFormId {
        file: file.parse().ok()?,
        object: u32::from_str_radix(object, 16).ok()?,
    })
}

#[cfg(test)]
mod edt_11_override_chain_dock {
    use std::fs;

    use soe_esm::plugin::{FileHeader, Record};
    use soe_esm::{Plugin, Tag};

    use super::{override_chain_entries, EditorDocument, GlobalFormId};

    #[test]
    fn edt_11_dock_entries_show_every_version_and_the_winner() {
        let directory = tempfile::tempdir().unwrap();
        let base_path = directory.path().join("Base.esm");
        let middle_path = directory.path().join("Middle.esp");
        let final_path = directory.path().join("Final.esp");

        let mut base = Plugin::new(FileHeader::default());
        base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "BaseStatue"));
        let mut middle = Plugin::new(FileHeader {
            masters: vec!["Base.esm".to_string()],
            ..Default::default()
        });
        middle.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "MiddleStatue"));
        let mut final_plugin = Plugin::new(FileHeader {
            masters: vec!["Base.esm".to_string(), "Middle.esp".to_string()],
            ..Default::default()
        });
        final_plugin.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "FinalStatue"));

        fs::write(&base_path, base.to_bytes()).unwrap();
        fs::write(&middle_path, middle.to_bytes()).unwrap();
        fs::write(&final_path, final_plugin.to_bytes()).unwrap();
        let document =
            EditorDocument::open(&[base_path, middle_path, final_path], "Final.esp").unwrap();
        let entries = override_chain_entries(
            &document,
            GlobalFormId {
                file: 0,
                object: 0x800,
            },
        );

        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.plugin_name.as_str())
                .collect::<Vec<_>>(),
            ["Base.esm", "Middle.esp", "Final.esp"]
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.signature.as_str())
                .collect::<Vec<_>>(),
            ["STAT", "STAT", "STAT"]
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.editor_id.as_str())
                .collect::<Vec<_>>(),
            ["BaseStatue", "MiddleStatue", "FinalStatue"]
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.is_winner)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
    }
}

#[cfg(test)]
mod edt_02_forms_dock {
    use std::fs;
    use std::path::PathBuf;

    use soe_editor::EditorDocument;
    use soe_esm::plugin::{FileHeader, Record};
    use soe_esm::records::refr::Refr;
    use soe_esm::records::Transform;
    use soe_esm::{GlobalFormId, Plugin, Tag};

    use super::{apply_ground_height, search_form_ids};

    const REFERENCE: GlobalFormId = GlobalFormId {
        file: 1,
        object: 0x800,
    };

    #[test]
    fn edt_02_dock_filters_paged_form_ids_without_building_resources() {
        let directory = tempfile::tempdir().unwrap();
        let base_path = directory.path().join("Base.esm");
        let patch_path = directory.path().join("Patch.esp");
        let mut base = Plugin::new(FileHeader::default());
        for index in 0..64 {
            let editor_id = format!("Stone{index}");
            base.push_top(
                Record::new(Tag(*b"STAT"), 0x800 + index).zsub(Tag(*b"EDID"), &editor_id),
            );
        }
        let patch = Plugin::new(FileHeader {
            masters: vec!["Base.esm".to_string()],
            ..Default::default()
        });
        fs::write(&base_path, base.to_bytes()).unwrap();
        fs::write(&patch_path, patch.to_bytes()).unwrap();
        let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();

        let ids = search_form_ids(&document, Some(Tag(*b"STAT")), "stone6", 0, 3);
        let next_page = search_form_ids(&document, Some(Tag(*b"STAT")), "stone6", 3, 3);

        assert_eq!(ids, vec!["0:000806", "0:00083C", "0:00083D"]);
        assert_eq!(next_page, vec!["0:00083E", "0:00083F"]);
        assert!(ids.iter().all(|id| id.contains(':')));
    }

    fn drop_fixture() -> (EditorDocument, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let base_path = directory.path().join("Base.esm");
        let active_path = directory.path().join("Active.esp");
        fs::write(&base_path, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
        let mut active = Plugin::new(FileHeader {
            masters: vec!["Base.esm".to_string()],
            ..Default::default()
        });
        let transform = Transform {
            position: [12.0, -8.0, 240.0],
            rotation: [0.1, 0.2, 0.3],
        };
        active.push_top(
            Record::new(Tag(*b"REFR"), 0x0100_0800)
                .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes().to_vec())
                .sub(Tag(*b"DATA"), transform.to_bytes().to_vec()),
        );
        fs::write(&active_path, active.to_bytes()).unwrap();
        let paths: [PathBuf; 2] = [base_path, active_path];
        let document = EditorDocument::open(&paths, "Active.esp").unwrap();
        (document, directory)
    }

    #[test]
    fn edt_15_collision_drop_edits_selected_reference_record_and_undoes() {
        let (mut document, _directory) = drop_fixture();
        let original = document
            .load_order()
            .winner(REFERENCE)
            .and_then(|record| Refr::from_record(record).ok())
            .and_then(|reference| reference.transform)
            .unwrap();

        apply_ground_height(&mut document, REFERENCE, -5.5).unwrap();

        let placed = document
            .load_order()
            .winner(REFERENCE)
            .and_then(|record| Refr::from_record(record).ok())
            .and_then(|reference| reference.transform)
            .unwrap();
        assert_eq!(placed.position, [12.0, -8.0, -5.5]);
        assert_eq!(placed.rotation, original.rotation);
        assert!(document.undo().unwrap());
        let undone = document
            .load_order()
            .winner(REFERENCE)
            .and_then(|record| Refr::from_record(record).ok())
            .and_then(|reference| reference.transform)
            .unwrap();
        assert_eq!(undone, original);
    }

    #[test]
    fn edt_15_non_finite_host_hit_does_not_edit_reference_record() {
        let (mut document, _directory) = drop_fixture();
        let original = document.active_plugin().clone();

        assert!(apply_ground_height(&mut document, REFERENCE, f32::NAN).is_err());

        assert_eq!(document.active_plugin(), &original);
    }
}
