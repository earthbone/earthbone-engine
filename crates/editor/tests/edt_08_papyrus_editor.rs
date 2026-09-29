// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::{EditorDocument, EditorError};
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};
use soe_papyrus::pex;

const OWNER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-08-{}-{label}", std::process::id()));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn document(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn source() -> &'static str {
    "ScriptName EditorProbe\nInt Property Count Auto\nString Property Label Auto\nFunction Ping()\nReturn\nEndFunction"
}

fn take_u16(bytes: &[u8], offset: &mut usize) -> u16 {
    let value = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
    *offset += 2;
    value
}

fn take_vmad_string(bytes: &[u8], offset: &mut usize) -> String {
    let length = take_u16(bytes, offset) as usize;
    let end = *offset + length * 2;
    let value = bytes[*offset..end]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
        .collect::<Vec<_>>();
    *offset = end;
    String::from_utf16(&value).unwrap()
}

#[test]
fn edt_08_script_compiles_and_binds_properties_to_owner() {
    let (mut document, base_path, patch_path) = document("bind");
    let (compiled, saved) = document
        .save_with_script(OWNER, "EditorProbe.psc", source(), &patch_path)
        .unwrap();
    let parsed = pex::parse(&compiled).unwrap();
    assert_eq!(parsed.objects[0].name, "EditorProbe");
    assert_eq!(parsed.objects[0].properties.len(), 2);
    assert!(parsed.objects[0].properties[0]
        .name
        .eq_ignore_ascii_case("Count"));

    assert_eq!(saved.records[0].form_id, 0x800);
    let vmad = saved.records[0].find(Tag(*b"VMAD")).unwrap();
    assert_eq!(&vmad[..6], &[5, 0, 2, 0, 1, 0]);
    let mut offset = 6;
    assert_eq!(take_vmad_string(vmad, &mut offset), "EditorProbe");
    assert_eq!(vmad[offset], 0);
    offset += 1;
    assert_eq!(take_u16(vmad, &mut offset), 2);
    assert_eq!(take_vmad_string(vmad, &mut offset), "Count");
    assert_eq!(&vmad[offset..offset + 2], &[3, 1]);
    offset += 2;
    assert_eq!(&vmad[offset..offset + 4], &[0, 0, 0, 0]);
    offset += 4;
    assert_eq!(take_vmad_string(vmad, &mut offset), "Label");
    assert_eq!(&vmad[offset..offset + 2], &[2, 1]);
    offset += 2;
    assert_eq!(take_u16(vmad, &mut offset), 0);
    assert_eq!(offset, vmad.len());
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_08_script_compile_failure_leaves_owner_unchanged() {
    let (mut document, base_path, patch_path) = document("reject");
    let before = document.active_plugin().clone();
    let error = document
        .compile_script_for_form(OWNER, "WrongName.psc", source())
        .unwrap_err();
    assert!(matches!(error, EditorError::Papyrus(_)));
    assert_eq!(document.active_plugin(), &before);
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_08_script_binding_rejects_existing_vmad_without_overwrite() {
    let (mut document, base_path, patch_path) = document("existing");
    document
        .set_subrecord(OWNER, Tag(*b"VMAD"), vec![1, 2, 3])
        .unwrap();
    let before = document.active_plugin().clone();
    let error = document
        .compile_script_for_form(OWNER, "EditorProbe.psc", source())
        .unwrap_err();
    assert!(matches!(error, EditorError::Papyrus(_)));
    assert_eq!(document.active_plugin(), &before);
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}
