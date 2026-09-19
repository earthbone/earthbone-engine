// SPDX-License-Identifier: GPL-3.0-or-later

//! Model selection shared by rendering and collision.
//!
//! UESP FURN and xEdit's wbRecord(FURN)/wbGenericModel define furniture's
//! MODL path and bit 23 editor-marker flag. Reuse the ESM string decoder,
//! without treating furniture as STAT or inventing a second record store.
//! UESP CONT defines the same MODL field for containers, including the
//! nightstands and cupboards that support placed candles.

use soe_esm::records::stat::Static;
use soe_esm::{tag, Record, Tag};

pub(crate) fn model_for(record: &Record) -> Option<String> {
    let model = if record.tag == tag::STAT {
        Static::from_record(record).ok()?.model?
    } else if (record.tag == Tag::new(b"FURN") && record.flags & (1 << 23) == 0)
        || record.tag == Tag::new(b"CONT")
    {
        soe_esm::cursor::zstring(record.find(Tag::new(b"MODL"))?)
    } else {
        return None;
    };
    (!model.is_empty()).then_some(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_furniture_and_container_models_share_selection() {
        for kind in [tag::STAT, Tag::new(b"FURN"), Tag::new(b"CONT")] {
            let record = Record::new(kind, 1).zsub(b"MODL", "Furniture/Bed.nif");
            assert_eq!(model_for(&record).as_deref(), Some("Furniture/Bed.nif"));
        }
    }

    #[test]
    fn furniture_editor_markers_and_empty_paths_are_not_scene_models() {
        let marker = Record::new(Tag::new(b"FURN"), 1)
            .with_flags(1 << 23)
            .zsub(b"MODL", "EditorMarker.nif");
        assert_eq!(model_for(&marker), None);
        for kind in [tag::STAT, Tag::new(b"FURN"), Tag::new(b"CONT")] {
            assert_eq!(model_for(&Record::new(kind, 1)), None);
            assert_eq!(model_for(&Record::new(kind, 1).zsub(b"MODL", "")), None);
        }
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
