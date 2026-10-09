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
//! UESP STAT also assigns bit 23 to static editor markers.
//! The cell loader removes shapes below those nodes, retaining ordinary
//! sibling geometry and leaving collision selection unchanged.

//! UESP ACTI and xEdit's wbRecord(ACTI) use the same MODL and marker flag.
//! xEdit's wbDefinitionsTES5.pas uses wbGenericModel for TREE, FLOR, LIGH,
//! MSTT, SCOL, TACT and loose items. ARMO instead uses MOD2 and MOD4 for
//! ground models. Its MODL fields are FormIDs of armor addons, never paths.

use soe_esm::records::stat::Static;
use soe_esm::{tag, Record, Tag};

/// xEdit identifies editor markers by the name on a `NiObjectNET` block.
pub(crate) fn is_editor_marker_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("editormarker")
}

pub(crate) fn model_for(record: &Record) -> Option<String> {
    let model = if record.tag == tag::STAT {
        Static::from_record(record).ok()?.model?
    } else if record.tag == Tag::new(b"ARMO") {
        [b"MOD2", b"MOD4"].into_iter().find_map(|field| {
            record
                .find(Tag::new(field))
                .map(soe_esm::cursor::zstring)
                .filter(|path| !path.is_empty())
        })?
    } else if ([Tag::new(b"ACTI"), Tag::new(b"FURN"), Tag::new(b"DOOR")].contains(&record.tag)
        && record.flags & (1 << 23) == 0)
        || matches!(
            record.tag.as_bytes(),
            b"CONT"
                | b"TREE"
                | b"FLOR"
                | b"LIGH"
                | b"MSTT"
                | b"SCOL"
                | b"TACT"
                | b"MISC"
                | b"WEAP"
                | b"BOOK"
                | b"INGR"
                | b"ALCH"
                | b"KEYM"
                | b"SLGM"
                | b"AMMO"
                | b"SCRL"
                | b"APPA"
        )
    {
        soe_esm::cursor::zstring(record.find(Tag::new(b"MODL"))?)
    } else if record.tag == tag::GRAS {
        soe_esm::records::gras::Grass::from_record(record)
            .ok()?
            .model?
    } else if record.tag == Tag::new(b"TREE") {
        soe_esm::cursor::zstring(record.find(Tag::new(b"MODL"))?)
    } else {
        return None;
    };
    (!model.is_empty()).then_some(model)
}

/// Visual selection excludes markers without changing collision model selection.
pub(super) fn render_model_for(record: &Record) -> Option<String> {
    (!is_marker_base(record))
        .then(|| model_for(record))
        .flatten()
}

/// Bit 23 means Is Marker only on these xEdit base definitions.
pub(super) fn is_marker_base(record: &Record) -> bool {
    matches!(record.tag.as_bytes(), b"STAT" | b"ACTI" | b"FURN" | b"DOOR")
        && record.flags & (1 << 23) != 0
}

/// xEdit's REFR definition identifies occlusion planes and room/portal data.
pub(super) fn is_editor_reference(record: &Record) -> bool {
    [b"XOCP", b"XORD", b"XRMR", b"XPOD", b"XPTL"]
        .iter()
        .any(|tag| record.find(Tag::new(tag)).is_some())
        || record
            .find(Tag::new(b"XPRM"))
            .and_then(|data| soe_esm::records::refr::Primitive::decode(data).ok())
            .is_some_and(|primitive| primitive.kind == 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_furniture_container_and_door_models_share_selection() {
        for kind in [
            tag::STAT,
            Tag::new(b"TREE"),
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
    fn rnd_32_placed_base_types_use_generic_models_for_rendering_and_collision() {
        for kind in [
            b"TREE", b"FLOR", b"LIGH", b"MSTT", b"SCOL", b"TACT", b"MISC", b"WEAP", b"BOOK",
            b"INGR", b"ALCH", b"KEYM", b"SLGM", b"AMMO", b"SCRL", b"APPA",
        ] {
            let record = Record::new(Tag::new(kind), 1).zsub(b"MODL", "Object.nif");
            assert_eq!(
                model_for(&record).as_deref(),
                Some("Object.nif"),
                "{kind:?}"
            );
            assert_eq!(render_model_for(&record).as_deref(), Some("Object.nif"));
            assert_eq!(model_for(&Record::new(Tag::new(kind), 1)), None);
            assert_eq!(
                model_for(&Record::new(Tag::new(kind), 1).zsub(b"MODL", "")),
                None
            );
        }
    }

    #[test]
    fn rnd_32_armor_uses_ground_models_and_never_addon_formids() {
        let record = Record::new(Tag::new(b"ARMO"), 1).sub(b"MODL", 0x123_u32.to_le_bytes());
        assert_eq!(model_for(&record), None);
        let female = record.zsub(b"MOD4", "Female.nif");
        assert_eq!(model_for(&female).as_deref(), Some("Female.nif"));
        let male = female.zsub(b"MOD2", "Male.nif");
        assert_eq!(model_for(&male).as_deref(), Some("Male.nif"));
    }

    #[test]
    fn rnd_32_marker_flag_only_applies_to_declared_marker_types() {
        for kind in [b"STAT", b"ACTI", b"DOOR", b"FURN"] {
            let record = Record::new(Tag::new(kind), 1)
                .with_flags(1 << 23)
                .zsub(b"MODL", "Marker.nif");
            assert_eq!(render_model_for(&record), None);
        }
        let weapon = Record::new(Tag::new(b"WEAP"), 1)
            .with_flags(1 << 23)
            .zsub(b"MODL", "Sword.nif");
        assert_eq!(render_model_for(&weapon).as_deref(), Some("Sword.nif"));
    }

    #[test]
    fn unsupported_records_and_malformed_statics_are_not_reinterpreted() {
        let sound = Record::new(Tag::new(b"SOUN"), 1).zsub(b"MODL", "SoundMarker.nif");
        assert_eq!(model_for(&sound), None);
        let malformed = Record::new(tag::STAT, 1)
            .zsub(b"MODL", "Wall.nif")
            .sub(b"OBND", [0u8; 3]);
        assert_eq!(model_for(&malformed), None);
    }
}

#[cfg(test)]
mod rnd_17_tests {
    use super::*;

    #[test]
    fn rnd_17_activator_models_render_but_editor_markers_do_not() {
        let record = Record::new(Tag::new(b"ACTI"), 1).zsub(b"MODL", "Barricade.nif");
        assert_eq!(model_for(&record).as_deref(), Some("Barricade.nif"));
        assert_eq!(model_for(&record.with_flags(1 << 23)), None);
    }
}

#[cfg(test)]
mod rnd_31_tests {
    use super::*;

    #[test]
    fn rnd_31_tree_records_select_their_authored_model_path() {
        let tree = Record::new(Tag::new(b"TREE"), 1).zsub(b"MODL", r"Landscape\Trees\Pine01.nif");
        assert_eq!(
            model_for(&tree).as_deref(),
            Some(r"Landscape\Trees\Pine01.nif")
        );
        assert_eq!(
            render_model_for(&tree).as_deref(),
            Some(r"Landscape\Trees\Pine01.nif")
        );
        assert_eq!(render_model_for(&tree.with_flags(1 << 23)), None);
    }
}
