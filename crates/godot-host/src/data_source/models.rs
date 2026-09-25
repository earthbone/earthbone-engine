// SPDX-License-Identifier: GPL-3.0-or-later

//! Model selection shared by rendering and collision.
//!
//! UESP FURN and xEdit's wbRecord(FURN)/wbGenericModel define furniture's
//! MODL path and bit 23 editor-marker flag. Reuse the ESM string decoder,
//! without treating furniture as STAT or inventing a second record store.
//! UESP CONT defines the same MODL field for containers, including the
//! nightstands and cupboards that support placed candles.
//! xEdit's wbRecord(DOOR) uses the generic MODL path and bit 23 marker flag too.
//! Its NIF reader also identifies `NiObjectNET` names containing `EditorMarker`.
//! The cell loader removes only shapes below those nodes, retaining ordinary
//! sibling geometry and leaving collision selection unchanged.

use soe_esm::records::stat::Static;
use soe_esm::{tag, Record, Tag};

/// xEdit identifies editor markers by the name on a `NiObjectNET` block.
pub(crate) fn is_editor_marker_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("editormarker")
}

pub(crate) fn model_for(record: &Record) -> Option<String> {
    let model = if record.tag == tag::STAT {
        Static::from_record(record).ok()?.model?
    } else if ([Tag::new(b"FURN"), Tag::new(b"DOOR")].contains(&record.tag)
        && record.flags & (1 << 23) == 0)
        || record.tag == Tag::new(b"CONT")
    {
        soe_esm::cursor::zstring(record.find(Tag::new(b"MODL"))?)
    } else if record.tag == tag::GRAS {
        soe_esm::records::gras::Grass::from_record(record)
            .ok()?
            .model?
    } else {
        return None;
    };
    (!model.is_empty()).then_some(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_furniture_container_and_door_models_share_selection() {
        for kind in [
            tag::STAT,
            Tag::new(b"FURN"),
            Tag::new(b"CONT"),
            Tag::new(b"DOOR"),
        ] {
            let record = Record::new(kind, 1).zsub(b"MODL", "Furniture/Bed.nif");
            assert_eq!(model_for(&record).as_deref(), Some("Furniture/Bed.nif"));
        }
    }

    #[test]
    fn editor_markers_and_empty_paths_are_not_scene_models() {
        for kind in [Tag::new(b"FURN"), Tag::new(b"DOOR")] {
            let marker = Record::new(kind, 1)
                .with_flags(1 << 23)
                .zsub(b"MODL", "EditorMarker.nif");
            assert_eq!(model_for(&marker), None);
        }
        for kind in [
            tag::STAT,
            Tag::new(b"FURN"),
            Tag::new(b"CONT"),
            Tag::new(b"DOOR"),
        ] {
            assert_eq!(model_for(&Record::new(kind, 1)), None);
            assert_eq!(model_for(&Record::new(kind, 1).zsub(b"MODL", "")), None);
        }
    }

    #[test]
    fn nif_editor_marker_names_are_case_insensitive() {
        assert!(is_editor_marker_name("EditorMarker"));
        assert!(is_editor_marker_name("xeditormarker01"));
        assert!(!is_editor_marker_name("WhiterunDoor"));
    }

    #[test]
    fn unsupported_records_and_malformed_statics_are_not_reinterpreted() {
        let weapon = Record::new(Tag::new(b"WEAP"), 1).zsub(b"MODL", "Sword.nif");
        assert_eq!(model_for(&weapon), None);
        let malformed = Record::new(tag::STAT, 1)
            .zsub(b"MODL", "Wall.nif")
            .sub(b"OBND", [0u8; 3]);
        assert_eq!(model_for(&malformed), None);
    }
}
