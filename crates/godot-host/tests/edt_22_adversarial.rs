// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../../nif/tests/common/mod.rs"]
mod nif_fixture;

use nif_fixture::{NifWriter, TestVertex};
use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_godot_host::facegen::{generate_npc_head, FaceGenError};

const NPC: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x900,
};
const HEAD: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const EXTRA: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};
const POINTS: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]];

fn head_nif(name: &str) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let vertices = [
        TestVertex::at(POINTS[0], [0.0, 0.0]),
        TestVertex::at(POINTS[1], [1.0, 0.0]),
        TestVertex::at(POINTS[2], [0.0, 1.0]),
    ];
    let data = writer.geometry_data(&vertices, true, &[[0, 1, 2]], None);
    let data = writer.add("NiTriShapeData", data);
    let shape = writer.ni_tri_shape(name, data, -1, -1, -1);
    let shape = writer.add("NiTriShape", shape);
    writer.roots.push(shape);
    writer.finish()
}

struct Fixture {
    directory: tempfile::TempDir,
    plugin: Plugin,
    assets: Vfs,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let meshes = directory.path().join("meshes");
        std::fs::create_dir(&meshes).unwrap();
        std::fs::write(meshes.join("Head.nif"), head_nif("Head")).unwrap();
        std::fs::write(meshes.join("Extra.nif"), head_nif("Extra")).unwrap();
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(b"HDPT", 0x800)
                .zsub(b"MODL", "Head.nif")
                .sub(b"HNAM", 0x801_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(b"HDPT", 0x801).zsub(b"MODL", "Extra.nif"));
        plugin.push_top(
            Record::new(b"NPC_", 0x900)
                .sub(b"PNAM", 0x800_u32.to_le_bytes())
                .sub(b"PNAM", 0x801_u32.to_le_bytes()),
        );
        let mut assets = Vfs::new();
        assets.add_loose_root(directory.path()).unwrap();
        Self {
            directory,
            plugin,
            assets,
        }
    }

    fn order(&self) -> LoadOrder {
        let mut order = LoadOrder::new();
        order
            .add(
                "Fixture.esp",
                Plugin::parse(&self.plugin.to_bytes()).unwrap(),
            )
            .unwrap();
        order
    }
}

#[test]
fn edt_22_repeated_and_nested_head_parts_are_emitted_once_in_depth_first_order() {
    let mut fixture = Fixture::new();
    let npc = fixture
        .plugin
        .records
        .iter_mut()
        .find(|record| record.tag.as_bytes() == b"NPC_")
        .unwrap();
    npc.subrecords
        .push(soe_esm::Subrecord::new(b"PNAM", 0x800_u32.to_le_bytes()));

    let order = fixture.order();
    let first = generate_npc_head(&order, &fixture.assets, NPC).unwrap();
    let second = generate_npc_head(&order, &fixture.assets, NPC).unwrap();
    let forms: Vec<_> = first.iter().map(|part| part.form).collect();
    assert_eq!(forms, [HEAD, EXTRA]);
    assert_eq!(
        second.iter().map(|part| part.form).collect::<Vec<_>>(),
        forms,
        "PNAM and HNAM overlap must not duplicate generated geometry"
    );
    assert_eq!(first[0].mesh.positions, POINTS);
    assert_eq!(first[1].mesh.positions, POINTS);
    assert!(fixture.directory.path().join("meshes/Head.nif").is_file());
}

#[test]
fn edt_22_missing_pnam_and_nam9_produce_no_generated_head_meshes() {
    let mut fixture = Fixture::new();
    let npc = fixture
        .plugin
        .records
        .iter_mut()
        .find(|record| record.tag.as_bytes() == b"NPC_")
        .unwrap();
    npc.subrecords
        .retain(|field| field.tag.as_bytes() != b"PNAM");

    let generated = generate_npc_head(&fixture.order(), &fixture.assets, NPC).unwrap();
    assert!(generated.is_empty());
}

#[test]
fn edt_22_orphan_nam1_is_rejected_before_any_partial_meshes_escape() {
    let mut fixture = Fixture::new();
    let head = fixture
        .plugin
        .records
        .iter_mut()
        .find(|record| record.form_id == 0x800)
        .unwrap();
    head.subrecords
        .push(soe_esm::Subrecord::new(b"NAM1", b"Orphan.tri\0".as_slice()));

    assert!(matches!(
        generate_npc_head(&fixture.order(), &fixture.assets, NPC),
        Err(FaceGenError::InvalidRecord { form: HEAD, .. })
    ));
}
