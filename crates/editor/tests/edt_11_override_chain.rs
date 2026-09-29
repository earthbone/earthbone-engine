// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const STAT: Tag = Tag(*b"STAT");
const SHARED_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const BASE_ONLY_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};
const MISSING_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x802,
};

struct Fixture {
    document: EditorDocument,
    paths: Vec<PathBuf>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = fs::remove_file(path);
        }
    }
}

fn fixture() -> Fixture {
    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
    let fixture_id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-11-{}-{fixture_id}",
        std::process::id()
    ));
    let paths = ["Base.esm", "Middle.esp", "Final.esp"]
        .into_iter()
        .map(|name| {
            root.with_file_name(format!(
                "{}-{name}",
                root.file_name().unwrap().to_string_lossy()
            ))
        })
        .collect::<Vec<_>>();

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "BaseVersion"));
    base.push_top(Record::new(STAT, 0x801).zsub(Tag(*b"EDID"), "BaseOnly"));

    let mut middle = Plugin::new(FileHeader {
        masters: vec![paths[0].file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    middle.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "MiddleVersion"));

    let mut final_plugin = Plugin::new(FileHeader {
        masters: paths[..2]
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect(),
        ..Default::default()
    });
    final_plugin.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "FinalVersion"));

    for (path, plugin) in paths.iter().zip([base, middle, final_plugin]) {
        fs::write(path, plugin.to_bytes()).unwrap();
    }
    let active_name = paths[2].file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(&paths, &active_name).unwrap();
    Fixture { document, paths }
}

#[test]
fn edt_11_selected_form_lists_each_version_in_load_order() {
    let fixture = fixture();
    let chain = fixture.document.override_chain(SHARED_FORM);

    assert_eq!(chain.len(), 3);
    assert_eq!(
        chain
            .iter()
            .map(|version| version.record.editor_id().unwrap())
            .collect::<Vec<_>>(),
        ["BaseVersion", "MiddleVersion", "FinalVersion"]
    );
    assert_eq!(
        chain
            .iter()
            .map(|version| version.plugin_name)
            .collect::<Vec<_>>(),
        [
            fixture.paths[0].file_name().unwrap().to_str().unwrap(),
            fixture.paths[1].file_name().unwrap().to_str().unwrap(),
            fixture.paths[2].file_name().unwrap().to_str().unwrap(),
        ]
    );
}

#[test]
fn edt_11_selected_form_marks_only_the_final_version_as_winner() {
    let fixture = fixture();
    let chain = fixture.document.override_chain(SHARED_FORM);

    assert_eq!(
        chain
            .iter()
            .map(|version| version.is_winner)
            .collect::<Vec<_>>(),
        [false, false, true]
    );
}

#[test]
fn edt_11_single_version_form_is_the_winner() {
    let fixture = fixture();
    let chain = fixture.document.override_chain(BASE_ONLY_FORM);

    assert_eq!(chain.len(), 1);
    assert!(chain[0].is_winner);
    assert_eq!(chain[0].record.editor_id().as_deref(), Some("BaseOnly"));
}

#[test]
fn edt_11_unknown_form_has_no_override_versions() {
    let fixture = fixture();

    assert!(fixture.document.override_chain(MISSING_FORM).is_empty());
}
