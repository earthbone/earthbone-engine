// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const STAT: Tag = Tag(*b"STAT");
const MISC: Tag = Tag(*b"MISC");
const BASE_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-02-{}-{label}", std::process::id()));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "BaseMarker"));
    base.push_top(Record::new(STAT, 0x801).zsub(Tag(*b"EDID"), "OtherStone"));
    base.push_top(Record::new(MISC, 0x802).zsub(Tag(*b"EDID"), "StoneMisc"));

    let mut patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    patch.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "WinningStone"));
    patch.records[0].form_id = 0x0000_0800;
    patch.push_top(Record::new(STAT, 0x0100_0803).zsub(Tag(*b"EDID"), "NewStone"));

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_02_filters_winning_forms_by_record_signature() {
    let (document, base, patch) = fixture("signature");
    let forms = document.search_forms(Some(STAT), "").collect::<Vec<_>>();

    assert_eq!(forms.len(), 3);
    assert!(forms.contains(&BASE_FORM));
    assert!(forms
        .iter()
        .all(|id| document.load_order().winner(*id).unwrap().tag == STAT));
    cleanup(&[base, patch]);
}

#[test]
fn edt_02_matches_editor_id_substrings_without_case_sensitivity() {
    let (document, base, patch) = fixture("query");
    let forms = document.search_forms(None, "wInNiNgSt").collect::<Vec<_>>();

    assert_eq!(forms, vec![BASE_FORM]);
    cleanup(&[base, patch]);
}

#[test]
fn edt_02_search_uses_the_winning_record_for_overrides() {
    let (document, base, patch) = fixture("winner");
    let forms = document
        .search_forms(Some(STAT), "WinningStone")
        .collect::<Vec<_>>();

    assert_eq!(forms, vec![BASE_FORM]);
    assert_eq!(
        document
            .load_order()
            .winner(forms[0])
            .unwrap()
            .editor_id()
            .as_deref(),
        Some("WinningStone")
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_02_search_yields_form_ids_for_lazy_record_lookup() {
    let (document, base, patch) = fixture("lazy");
    let mut forms = document.search_forms(None, "stone");

    assert_eq!(forms.next(), Some(BASE_FORM));
    let selected = forms.next().unwrap();
    assert_eq!(
        document
            .load_order()
            .winner(selected)
            .unwrap()
            .editor_id()
            .as_deref(),
        Some("OtherStone")
    );
    cleanup(&[base, patch]);
}
