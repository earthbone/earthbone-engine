// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot bridge for selected placed-reference transform edits.
//!
//! The editor gizmo edits a proxy Node3D, while this bridge applies its
//! transform to the active plugin's REFR record through `soe-editor`.

// CONTEXT: gdext's class(init) expansion emits explicit field names.
#![allow(clippy::redundant_field_names)]

use std::path::PathBuf;

use godot::classes::RefCounted;
use godot::prelude::*;
use soe_editor::EditorDocument;
use soe_esm::records::Transform;
use soe_esm::GlobalFormId;

#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct SoeReferenceTransformEditor {
    base: Base<RefCounted>,
    document: Option<EditorDocument>,
    active_path: Option<PathBuf>,
    last_error: GString,
}

#[godot_api]
impl SoeReferenceTransformEditor {
    /// Opens an ordered plugin list and selects the plugin that receives edits.
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
                self.active_path = paths
                    .iter()
                    .find(|path| {
                        path.file_name().is_some_and(|name| {
                            name.to_string_lossy()
                                .eq_ignore_ascii_case(&active_plugin.to_string())
                        })
                    })
                    .cloned();
                self.document = Some(document);
                self.last_error = GString::new();
                true
            }
            Err(error) => {
                self.document = None;
                self.active_path = None;
                self.last_error = GString::from(error.to_string().as_str());
                false
            }
        }
    }

    /// Returns the current record position and Euler rotation as a dictionary.
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
        let Ok(reference) = soe_esm::records::refr::Refr::from_record(record) else {
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

    /// Applies one gizmo transform and appends it to the editor document undo log.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn set_reference_transform(
        &mut self,
        encoded_id: GString,
        position: Vector3,
        rotation: Vector3,
    ) -> bool {
        let Some(document) = self.document.as_mut() else {
            return false;
        };
        let Some(id) = parse_form_id(&encoded_id.to_string()) else {
            return false;
        };
        let transform = Transform {
            position: [position.x, position.y, position.z],
            rotation: [rotation.x, rotation.y, rotation.z],
        };
        match document.set_reference_transform(id, transform) {
            Ok(()) => true,
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                false
            }
        }
    }

    /// Reverts the most recent document edit.
    #[func]
    pub fn undo(&mut self) -> bool {
        self.document
            .as_mut()
            .and_then(|document| document.undo().ok())
            .unwrap_or(false)
    }

    /// Writes the active plugin with all committed document edits.
    #[func]
    pub fn save(&self) -> bool {
        let (Some(document), Some(path)) = (self.document.as_ref(), self.active_path.as_ref())
        else {
            return false;
        };
        match document.save(path) {
            Ok(_) => true,
            Err(error) => {
                godot_error!("SoeReferenceTransformEditor: {error}");
                false
            }
        }
    }

    /// Returns the most recent document diagnostic.
    #[func]
    pub fn last_error(&self) -> GString {
        self.last_error.clone()
    }
}

fn parse_form_id(value: &str) -> Option<GlobalFormId> {
    let (file, object) = value.split_once(':')?;
    Some(GlobalFormId {
        file: file.parse().ok()?,
        object: u32::from_str_radix(object, 16).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_form_id;
    use soe_esm::GlobalFormId;

    #[test]
    fn edt_14_form_id_parser_accepts_encoded_reference_and_rejects_ambiguous_ids() {
        assert_eq!(
            parse_form_id("1:000800"),
            Some(GlobalFormId {
                file: 1,
                object: 0x800
            })
        );
        for invalid in ["", "1", "x:000800", "1:not-hex", "1:800:tail"] {
            assert_eq!(parse_form_id(invalid), None, "{invalid:?}");
        }
    }
}
