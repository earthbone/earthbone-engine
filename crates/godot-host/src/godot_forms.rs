// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot editor bridge for the load-order-backed forms dock.
//!
//! The dock sends a signature and text query to `soe-editor`, which returns
//! winning FormIDs. A Godot Resource is created only when the dock selects an
//! ID, following Godot's EditorPlugin and EditorInspectorPlugin APIs.

use std::path::PathBuf;

use godot::classes::RefCounted;
use godot::prelude::*;
use soe_editor::EditorDocument;
use soe_esm::{GlobalFormId, Tag};

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

    /// Count selected-form Resources created since the last successful open.
    #[func]
    pub fn resources_created(&self) -> i64 {
        self.resources_created
    }
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
mod edt_02_forms_dock {
    use std::fs;

    use soe_editor::EditorDocument;
    use soe_esm::plugin::{FileHeader, Record};
    use soe_esm::{Plugin, Tag};

    use super::search_form_ids;

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
}
