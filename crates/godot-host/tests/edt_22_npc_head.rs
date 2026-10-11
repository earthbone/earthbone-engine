// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Record-to-render checks use xEdit NPC_/HDPT layouts and independent NIF fixtures.

#[path = "../../nif/tests/common/mod.rs"]
mod nif_fixture;

use std::path::{Path, PathBuf};

use nif_fixture::{Buf, NifWriter, TestVertex};
use soe_bsa::Vfs;
use soe_esm::records::npc::FACE_MORPH_COUNT;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_godot_host::facegen::{generate_npc_head, nam9_morph_weights, FaceGenError};
use soe_godot_host::units::{SkyrimPos, METRES_PER_UNIT};

const NPC: GlobalFormId = GlobalFormId {
    file: 1,
    object: 0x900,
};
const HEAD: GlobalFormId = GlobalFormId {
    file: 1,
    object: 0x800,
};
const EXTRA: GlobalFormId = GlobalFormId {
    file: 2,
    object: 0x801,
};
const POINTS: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]];
const EXPECTED: [[f32; 3]; 3] = [[2.5, 0.0, 5.0], [105.0, 0.0, 5.0], [2.5, 100.0, 10.0]];

fn head_nif(name: &str, points: &[[f32; 3]; 3]) -> Vec<u8> {
    head_nif_shapes(&[(name, *points)])
}

fn head_nif_shapes(shapes: &[(&str, [[f32; 3]; 3])]) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    for (name, points) in shapes {
        let vertices = [
            TestVertex::at(points[0], [0.0, 0.0]),
            TestVertex::at(points[1], [1.0, 0.0]),
            TestVertex::at(points[2], [0.0, 1.0]),
        ];
        let data = writer.geometry_data(&vertices, true, &[[0, 1, 2]], None);
        let data = writer.add("NiTriShapeData", data);
        let shape = writer.ni_tri_shape(name, data, -1, -1, -1);
        let shape = writer.add("NiTriShape", shape);
        writer.roots.push(shape);
    }
    writer.finish()
}

fn head_tri() -> Vec<u8> {
    let mut out = Buf(b"FRTRI003".to_vec());
    for value in [3, 1, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0] {
        out.u32(value);
    }
    for point in POINTS {
        out.vec3(point);
    }
    for index in [0, 1, 2, 0, 0, 0] {
        out.u32(index);
    }
    for (name, deltas) in [
        ("JawWide", [[1000_i16, 0, 0], [2000, 0, 0], [1000, 0, 0]]),
        ("NoseShort", [[0, 0, 1000], [0, 0, 1000], [0, 0, 2000]]),
        ("NoseLong", [[0, 0, -3000]; 3]),
    ] {
        out.sized_string(name).f32(0.01);
        for delta in deltas {
            for component in delta {
                out.bytes(&component.to_le_bytes());
            }
        }
    }
    out.0
}

fn npc_record(morph: &[f32; FACE_MORPH_COUNT]) -> Record {
    Record::new(b"NPC_", 0x900)
        .sub(b"PNAM", 0x800_u32.to_le_bytes())
        .sub(b"PNAM", 0x0100_0801_u32.to_le_bytes())
        .sub(
            b"NAM9",
            morph
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>(),
        )
}

struct Fixture {
    directory: tempfile::TempDir,
    unrelated: Plugin,
    base: Plugin,
    patch: Plugin,
    assets: Vfs,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let meshes = directory.path().join("meshes");
        std::fs::create_dir(&meshes).unwrap();
        std::fs::write(meshes.join("Head.nif"), head_nif("Head", &POINTS)).unwrap();
        std::fs::write(meshes.join("Extra.nif"), head_nif("Extra", &POINTS)).unwrap();
        std::fs::write(meshes.join("HeadChargen.tri"), head_tri()).unwrap();
        let mut unrelated = Plugin::new(FileHeader::default());
        unrelated.push_top(Record::new(b"WEAP", 0x800));
        let mut base = Plugin::new(FileHeader::default());
        base.push_top(Record::new(b"HDPT", 0x800).zsub(b"MODL", "losing.nif"));
        base.push_top(Record::new(b"NPC_", 0x900).sub(b"PNAM", 0x800_u32.to_le_bytes()));
        let mut patch = Plugin::new(FileHeader {
            masters: vec!["Base.esm".into()],
            ..Default::default()
        });
        patch.push_top(
            Record::new(b"HDPT", 0x800)
                .zsub(b"MODL", "HEAD.nif")
                .sub(b"NAM0", 1_u32.to_le_bytes())
                .zsub(b"NAM1", "expression-not-chargen.tri")
                .sub(b"NAM0", 2_u32.to_le_bytes())
                .zsub(b"NAM1", "HeadChargen.tri")
                .sub(b"HNAM", 0x0100_0801_u32.to_le_bytes()),
        );
        patch.push_top(Record::new(b"HDPT", 0x0100_0801).zsub(b"MODL", "Meshes\\Extra.nif"));
        let mut morph = [0.0; FACE_MORPH_COUNT];
        morph[0] = -0.5;
        morph[3] = 0.25;
        patch.push_top(npc_record(&morph));
        let mut assets = Vfs::new();
        assets.add_loose_root(directory.path()).unwrap();
        Self {
            directory,
            unrelated,
            base,
            patch,
            assets,
        }
    }

    fn order(&self) -> LoadOrder {
        let mut order = LoadOrder::new();
        for (name, plugin) in [
            ("Unrelated.esm", &self.unrelated),
            ("Base.esm", &self.base),
            ("Patch.esp", &self.patch),
        ] {
            order
                .add(name, Plugin::parse(&plugin.to_bytes()).unwrap())
                .unwrap();
        }
        order
    }

    fn write_plugins(&self) {
        for (name, plugin) in [
            ("Unrelated.esm", &self.unrelated),
            ("Base.esm", &self.base),
            ("Patch.esp", &self.patch),
        ] {
            std::fs::write(self.directory.path().join(name), plugin.to_bytes()).unwrap();
        }
    }

    fn record_mut(&mut self, tag: [u8; 4], id: u32) -> &mut Record {
        self.patch
            .records
            .iter_mut()
            .find(|record| record.tag == Tag(tag) && record.form_id == id)
            .unwrap()
    }
}

#[test]
fn edt_22_npc_pnam_and_nam9_generate_matching_render_geometry() {
    let fixture = Fixture::new();
    let order = fixture.order();
    let generated = generate_npc_head(&order, &fixture.assets, NPC).unwrap();
    assert_eq!(
        generated.iter().map(|part| part.form).collect::<Vec<_>>(),
        [HEAD, EXTRA]
    );
    let head = &generated[0];
    assert_eq!(
        head.mesh.positions, EXPECTED,
        "NAM9 sign and magnitude select named deltas, independently of TRI order"
    );
    assert_eq!(
        generated[1].mesh.positions, POINTS,
        "an extra without chargen TRI retains base geometry"
    );
    assert_eq!(head.mesh.indices, [0, 1, 2]);
    assert_eq!(head.surface.indices, [0, 2, 1]);
    assert_eq!(head.mesh.uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    assert_eq!(head.surface.uvs, head.mesh.uvs);
    assert_eq!(head.surface.triangle_count(), 1);
    for (actual, expected) in head.surface.positions.iter().zip(EXPECTED) {
        let expected = SkyrimPos::new(expected[0], expected[1], expected[2]).to_godot();
        assert_eq!(*actual, [expected.x, expected.y, expected.z]);
    }
    assert!(head
        .surface
        .normals
        .iter()
        .flatten()
        .all(|component| component.is_finite()));
    assert!(head.surface.bounds.max.y >= 10.0 * METRES_PER_UNIT);
    assert_eq!(
        generate_npc_head(&order, &fixture.assets, NPC).unwrap()[0].mesh,
        head.mesh,
        "generation does not accumulate morphs"
    );
    assert_eq!(
        soe_nif::parse(&fixture.assets.read("meshes/head.nif").unwrap())
            .unwrap()
            .meshes()
            .unwrap()[0]
            .positions,
        POINTS
    );
}

#[test]
fn edt_22_all_nam9_slots_select_signed_targets_and_default_is_neutral() {
    let names = [
        ("NoseShort", "NoseLong"),
        ("NoseDown", "NoseUp"),
        ("JawUp", "JawDown"),
        ("JawNarrow", "JawWide"),
        ("JawBack", "JawForward"),
        ("CheeksDown", "CheeksUp"),
        ("CheeksIn", "CheeksOut"),
        ("EyesMoveDown", "EyesMoveUp"),
        ("EyesMoveIn", "EyesMoveOut"),
        ("BrowDown", "BrowUp"),
        ("BrowIn", "BrowOut"),
        ("BrowBack", "BrowForward"),
        ("LipMoveDown", "LipMoveUp"),
        ("LipMoveIn", "LipMoveOut"),
        ("ChinThin", "ChinWide"),
        ("ChinMoveUp", "ChinMoveDown"),
        ("Overbite", "Underbite"),
        ("EyesBack", "EyesForward"),
        ("VampireMorph", "VampireMorph"),
    ];
    for (index, (lower, upper)) in names.into_iter().enumerate() {
        for (value, expected) in [(-0.5, lower), (0.25, upper)] {
            let mut values = [0.0; FACE_MORPH_COUNT];
            values[index] = value;
            assert_eq!(
                nam9_morph_weights(&values).unwrap(),
                [(expected, value.abs())],
                "NAM9 slot {index}"
            );
        }
    }
    assert!(nam9_morph_weights(&[0.0; FACE_MORPH_COUNT])
        .unwrap()
        .is_empty());
    assert!(nam9_morph_weights(&[f32::MAX; FACE_MORPH_COUNT])
        .unwrap()
        .is_empty());
    assert!(nam9_morph_weights(&[f32::NAN; FACE_MORPH_COUNT]).is_err());
}

#[test]
fn edt_22_zero_nam9_restores_declared_geometry() {
    let mut fixture = Fixture::new();
    *fixture.record_mut(*b"NPC_", 0x900) = npc_record(&[0.0; FACE_MORPH_COUNT]);
    let generated = generate_npc_head(&fixture.order(), &fixture.assets, NPC).unwrap();
    assert_eq!(generated[0].mesh.positions, POINTS);
    assert!(!generated[0].mesh.normals.is_empty());
}

#[test]
fn edt_22_part_without_selected_targets_keeps_its_geometry() {
    let mut fixture = Fixture::new();
    let mut values = [0.0; FACE_MORPH_COUNT];
    values[11] = 0.5;
    *fixture.record_mut(*b"NPC_", 0x900) = npc_record(&values);
    let generated = generate_npc_head(&fixture.order(), &fixture.assets, NPC).unwrap();
    assert_eq!(
        generated[0].mesh.positions, POINTS,
        "absent BrowForward cannot select a different TRI morph"
    );
}

#[test]
fn edt_22_only_the_unique_matching_nif_shape_is_morphed() {
    let fixture = Fixture::new();
    let mut accessory = POINTS;
    for point in &mut accessory {
        point[2] += 20.0;
    }
    std::fs::write(
        fixture.directory.path().join("meshes/Head.nif"),
        head_nif_shapes(&[("Accessory", accessory), ("Head", POINTS)]),
    )
    .unwrap();

    let generated = generate_npc_head(&fixture.order(), &fixture.assets, NPC).unwrap();
    assert_eq!(generated.len(), 3);
    assert_eq!(generated[0].form, HEAD);
    assert_eq!(generated[0].mesh.name, "Accessory");
    assert_eq!(generated[0].mesh.positions, accessory);
    assert!(!generated[0].mesh.normals.is_empty());
    assert_eq!(generated[1].form, HEAD);
    assert_eq!(generated[1].mesh.name, "Head");
    assert_eq!(generated[1].mesh.positions, EXPECTED);
    assert!(generated[1].mesh.normals.is_empty());
    assert_eq!(generated[1].surface.normals.len(), 3);
    assert_eq!(generated[2].form, EXTRA);
}

#[test]
fn edt_22_head_assets_follow_the_supplied_vfs_precedence() {
    let mut fixture = Fixture::new();
    let mut replacement = POINTS;
    replacement[1][0] += 25.0;
    let overlay = fixture.directory.path().join("overlay");
    std::fs::create_dir_all(overlay.join("meshes")).unwrap();
    std::fs::write(
        overlay.join("meshes/Extra.nif"),
        head_nif("ReplacementExtra", &replacement),
    )
    .unwrap();
    fixture.assets.add_loose_root(&overlay).unwrap();

    let generated = generate_npc_head(&fixture.order(), &fixture.assets, NPC).unwrap();
    assert_eq!(generated[0].mesh.positions, EXPECTED);
    assert_eq!(generated[1].mesh.name, "ReplacementExtra");
    assert_eq!(generated[1].mesh.positions, replacement);
}

#[test]
fn edt_22_ambiguous_tri_shape_binding_is_rejected() {
    let fixture = Fixture::new();
    let mut file = soe_nif::parse(&head_nif("Head", &POINTS)).unwrap();
    let shape = file.blocks.last().unwrap().clone();
    file.roots.push(soe_nif::BlockRef(file.blocks.len() as i32));
    file.blocks.push(shape);
    std::fs::write(
        fixture.directory.path().join("meshes/Head.nif"),
        file.to_bytes().unwrap(),
    )
    .unwrap();
    assert!(matches!(
        generate_npc_head(&fixture.order(), &fixture.assets, NPC),
        Err(FaceGenError::InvalidRecord { form: HEAD, .. })
    ));
}

#[test]
fn edt_22_malformed_nam9_returns_record_error() {
    let mut fixture = Fixture::new();
    let record = fixture.record_mut(*b"NPC_", 0x900);
    record.subrecords.last_mut().unwrap().data.pop();
    assert!(matches!(
        generate_npc_head(&fixture.order(), &fixture.assets, NPC),
        Err(FaceGenError::Record(_))
    ));
}

#[test]
fn edt_22_bad_links_deleted_parts_and_cycles_return_errors() {
    for case in 0..5 {
        let mut fixture = Fixture::new();
        match case {
            0 => {
                fixture.record_mut(*b"NPC_", 0x900).subrecords[0].data =
                    0x999_u32.to_le_bytes().to_vec();
            }
            1 => fixture.record_mut(*b"HDPT", 0x800).tag = Tag(*b"WEAP"),
            2 => fixture.record_mut(*b"HDPT", 0x800).flags |= soe_esm::plugin::flags::DELETED,
            3 => fixture
                .record_mut(*b"HDPT", 0x0100_0801)
                .subrecords
                .push(soe_esm::Subrecord::new(b"HNAM", 0x800_u32.to_le_bytes())),
            _ => fixture.record_mut(*b"NPC_", 0x900).subrecords[0].data = vec![0; 3],
        }
        assert!(
            generate_npc_head(&fixture.order(), &fixture.assets, NPC).is_err(),
            "invalid link case {case}"
        );
    }
}

#[test]
fn edt_22_malformed_asset_declarations_and_mismatched_tri_return_errors() {
    for case in 0..5 {
        let mut fixture = Fixture::new();
        match case {
            0 => {
                fixture.record_mut(*b"HDPT", 0x800).subrecords.remove(4);
            }
            1 => {
                fixture.record_mut(*b"HDPT", 0x800).subrecords.remove(3);
            }
            2 => {
                let mut points = POINTS;
                points[1][0] += 0.5;
                std::fs::write(
                    fixture.directory.path().join("meshes/Head.nif"),
                    head_nif("Head", &points),
                )
                .unwrap();
            }
            3 => {
                std::fs::write(
                    fixture.directory.path().join("meshes/HeadChargen.tri"),
                    b"truncated",
                )
                .unwrap();
            }
            _ => {
                std::fs::remove_file(fixture.directory.path().join("meshes/Head.nif")).unwrap();
            }
        }
        assert!(
            generate_npc_head(&fixture.order(), &fixture.assets, NPC).is_err(),
            "invalid asset case {case}"
        );
    }
}

#[test]
fn edt_22_npc_head_surfaces_render_in_godot() {
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let fixture = Fixture::new();
    fixture.write_plugins();
    let output = std::process::Command::new(binary)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "res://edt_22_facegen_mesh_test.gd",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .env("EDT_22_FIXTURE", fixture.directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Godot render check failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("EDT_22_RENDER_OK"));
    assert!(fixture.directory.path().join("edt_22_face.png").is_file());
}

#[test]
fn edt_22_vanilla_npc_head_parts_use_declared_chargen_tri() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        return;
    };
    let mut order = LoadOrder::new();
    let mut assets = Vfs::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(directory.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
        for path in soe_bsa::find_associated_archives(name, &directory).unwrap() {
            assets.add_archive_path(&path).unwrap();
        }
    }
    assets.add_loose_root(&directory).unwrap();
    let (npc, _) = order
        .winners_of(Tag(*b"NPC_"))
        .find(|(_, record)| record.editor_id().as_deref() == Some("Lydia"))
        .expect("vanilla Lydia NPC");
    let generated = generate_npc_head(&order, &assets, npc).unwrap();
    assert!(!generated.is_empty());
    let mut changed = false;
    for part in &generated {
        assert_eq!(order.winner(part.form).unwrap().tag, Tag(*b"HDPT"));
        assert_eq!(part.surface.positions.len(), part.mesh.positions.len());
        assert_eq!(part.surface.triangle_count(), part.mesh.triangle_count());
        let record = order.winner(part.form).unwrap();
        let model = soe_esm::cursor::zstring(record.find(Tag(*b"MODL")).unwrap());
        let path = model.replace('\\', "/");
        let path = if path.to_ascii_lowercase().starts_with("meshes/") {
            path
        } else {
            format!("meshes/{path}")
        };
        let original = soe_nif::parse(&assets.read(&path).unwrap())
            .unwrap()
            .meshes()
            .unwrap();
        let original = original
            .iter()
            .find(|mesh| mesh.block == part.mesh.block)
            .unwrap();
        changed |= original.positions != part.mesh.positions;
    }
    assert!(
        changed,
        "Lydia's declared NAM9 values must change chargen head geometry"
    );
}
