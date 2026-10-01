// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-08: memory image preservation, edits and malformed relocation rejection.

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::document::{Address, Fixup, PackfileDocument};
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Error, Packfile};

fn fixture() -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let root = b.object("hkRootLevelContainer", 32);
    let opaque = b.object("ModOpaqueClass", 32);
    b.set_u32(opaque, 16, 0x12345678);
    b.ptr(root, 0, opaque);
    b.string(root, 8, "old");
    b.finish()
}

fn behavior_model(pf: &Packfile) -> (BehaviorGraph, Vec<[[u32; 4]; 2]>) {
    let mut graph = BehaviorGraph::from_packfile(pf).unwrap();
    let mut output_bits = Vec::new();
    for node in &mut graph.nodes {
        if let NodeKind::FootIkControls { controls, .. } = &mut node.kind {
            // CONTEXT: Vanilla foot IK output vectors can contain NaNs.
            output_bits.push([
                controls.error_out_translation.map(f32::to_bits),
                controls.align_with_ground_rotation.map(f32::to_bits),
            ]);
            controls.error_out_translation = [0.0; 4];
            controls.align_with_ground_rotation = [0.0; 4];
        }
    }
    (graph, output_bits)
}

#[test]
fn behavior_roundtrip_preserves_and_distinguishes_nan_output_bits() {
    let foot_ik_fixture = |nan_bits| {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let graph = b.object("hkbBehaviorGraph", 304);
        let variant = b.array_structs(root, 0, 24, 1);
        b.string(variant, 0, "hkbBehaviorGraph");
        b.string(variant, 8, "hkbBehaviorGraph");
        b.ptr(variant, 16, graph);
        let wrapper = b.object("hkbModifierGenerator", 88);
        let modifier = b.object("hkbFootIkControlsModifier", 176);
        b.ptr(graph, 128, wrapper);
        b.ptr(wrapper, 72, modifier);
        b.set_bool(modifier, 72, true);
        b.set_u32(modifier, 152, nan_bits);
        b.finish()
    };
    let source = foot_ik_fixture(0x7fc0_0001);
    let doc = PackfileDocument::parse(&source).unwrap();
    let before = Packfile::parse(&source).unwrap();
    let after = Packfile::parse(&doc.to_bytes().unwrap()).unwrap();
    assert_eq!(behavior_model(&before), behavior_model(&after));
    let changed = Packfile::parse(&foot_ik_fixture(0x7fc0_0002)).unwrap();
    assert_ne!(behavior_model(&before), behavior_model(&changed));
}

#[test]
fn opaque_objects_and_relocations_survive_rebuilt_file() {
    let original = fixture();
    let doc = PackfileDocument::parse(&original).unwrap();
    let rewritten = doc.to_bytes().unwrap();
    assert_eq!(PackfileDocument::parse(&rewritten).unwrap(), doc);
}

#[test]
fn edits_to_fields_strings_and_targets_reach_reread() {
    let mut doc = PackfileDocument::parse(&fixture()).unwrap();
    let s = &mut doc.sections[2];
    s.data[48..52].copy_from_slice(&0x87654321u32.to_le_bytes());
    let appended = s.data.len() as u32;
    s.data.extend_from_slice(b"a longer string\0");
    s.local[0].target = appended;
    s.global[0].target.offset = 0;
    let bytes = doc.to_bytes().unwrap();
    let pf = Packfile::parse(&bytes).unwrap();
    assert_eq!(pf.object(32).u32(16).unwrap(), 0x87654321);
    assert_eq!(
        pf.root().unwrap().string(8).unwrap().as_deref(),
        Some("a longer string")
    );
    assert_eq!(pf.root().unwrap().ptr_required(0, "self").unwrap().base, 0);
    assert_eq!(pf.object(32).class(), Some("ModOpaqueClass"));
}

#[test]
fn another_section_and_its_relocations_survive() {
    let mut doc = PackfileDocument::parse(&fixture()).unwrap();
    doc.sections[1].data = vec![0x53; 16];
    doc.sections[1].global.push(Fixup {
        source: 0,
        target: Address {
            section: 2,
            offset: 32,
        },
    });
    doc.sections[2].global[0].target = Address {
        section: 1,
        offset: 8,
    };
    let reread = PackfileDocument::parse(&doc.to_bytes().unwrap()).unwrap();
    assert_eq!(reread, doc);
}

#[test]
fn invalid_edits_fail_before_serialization() {
    let original = PackfileDocument::parse(&fixture()).unwrap();
    for case in 0..8 {
        let mut doc = original.clone();
        let s = &mut doc.sections[2];
        match case {
            0 => s.local[0].source = s.data.len() as u32 - 1,
            1 => s.local[0].target = s.data.len() as u32,
            2 => s.global[0].target.section = 99,
            3 => s.global[0].target.offset = s.data.len() as u32,
            4 => s.virtual_[0].target.section = 2,
            5 => s.local.push(s.local[0]),
            6 => s.global[0].source = s.local[0].source + 1,
            _ => s.virtual_.push(s.virtual_[0]),
        }
        assert!(
            matches!(doc.to_bytes(), Err(Error::BadFixup { .. })),
            "case {case}"
        );
    }
}

fn patch(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn malformed_file_relocations_and_section_ranges_are_rejected() {
    let original = fixture();
    let pf = Packfile::parse(&original).unwrap();
    let s = &pf.sections[2];
    let start = s.absolute_data_start as usize;
    let local = start + s.local_fixups_offset as usize;
    let global = start + s.global_fixups_offset as usize;
    let virtual_ = start + s.virtual_fixups_offset as usize;
    for (at, value) in [
        (local, s.local_fixups_offset - 1),
        (local + 4, s.local_fixups_offset),
        (global + 4, 99),
        (global + 8, s.local_fixups_offset),
        (virtual_ + 4, 2),
        (virtual_ + 8, 1),
        (local + 8, 0),
        (64 + 2 * 48 + 40, u32::MAX),
        (64 + 48 + 20, 0),
        (24, 99),
    ] {
        let mut bytes = original.clone();
        patch(&mut bytes, at, value);
        assert!(PackfileDocument::parse(&bytes).is_err(), "offset {at}");
    }
}

#[test]
fn an_empty_displaced_section_table_returns_an_error() {
    let mut bytes = fixture();
    patch(&mut bytes, 20, 0);
    bytes[62..64].copy_from_slice(&i16::MAX.to_le_bytes());
    assert!(matches!(
        PackfileDocument::parse(&bytes),
        Err(Error::Truncated { .. })
    ));
}

#[test]
fn truncated_files_never_panic() {
    let bytes = fixture();
    for len in 0..bytes.len() {
        assert!(
            PackfileDocument::parse(&bytes[..len]).is_err(),
            "length {len}"
        );
    }
}

#[test]
fn pointer_slot_validation_uses_the_file_pointer_width() {
    let mut source = fixture();
    source[16] = 4;
    let mut doc = PackfileDocument::parse(&source).unwrap();
    let len = doc.sections[2].data.len() as u32;
    doc.sections[2].local[0].source = len - 4;
    assert!(doc.to_bytes().is_ok());
    doc.sections[2].local[0].source = len - 3;
    assert!(matches!(doc.to_bytes(), Err(Error::BadFixup { .. })));
}

#[test]
fn installed_animation_edit_changes_modeled_duration() {
    use soe_hkx::animation::SplineCompressedAnimation;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping animation edit");
        return;
    };
    let archive = soe_bsa::Archive::open(&data.join("Skyrim - Animations.bsa")).unwrap();
    let bytes = archive
        .read("meshes/actors/character/animations/male/mt_idle.hkx")
        .unwrap();
    let pf = Packfile::parse(&bytes).unwrap();
    let object = pf.objects_of("hkaSplineCompressedAnimation")[0];
    let mut expected = SplineCompressedAnimation::from_obj(object).unwrap();
    expected.duration *= 2.0;
    let mut doc = PackfileDocument::parse(&bytes).unwrap();
    let section = pf
        .sections
        .iter()
        .position(|s| s.tag == "__data__")
        .unwrap();
    let field = object.base as usize + 20;
    doc.sections[section].data[field..field + 4].copy_from_slice(&expected.duration.to_le_bytes());
    let reread = Packfile::parse(&doc.to_bytes().unwrap()).unwrap();
    assert_eq!(
        SplineCompressedAnimation::from_obj(reread.object(object.base)).unwrap(),
        expected
    );
}

#[test]
fn anm_08_installed_packfiles_roundtrip() {
    use soe_hkx::animation::SplineCompressedAnimation;
    use soe_hkx::character::{CharacterData, ProjectData};
    use soe_hkx::skeleton::AnimationContainer;
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-08 corpus");
        return;
    };
    let mut archives: Vec<_> = std::fs::read_dir(data)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsa"))
                    && (n.starts_with("Skyrim - ")
                        || [
                            "Dawnguard.bsa",
                            "HearthFires.bsa",
                            "Dragonborn.bsa",
                            "Update.bsa",
                        ]
                        .contains(&n))
            })
        })
        .collect();
    archives.sort();
    let mut files = 0;
    let mut modeled = [0usize; 5];
    let mut opaque = 0;
    for path in archives {
        let archive = soe_bsa::Archive::open(&path).unwrap();
        let mut names: Vec<_> = archive
            .files()
            .iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".hkx"))
            .collect();
        names.sort();
        let mut count = 0;
        for name in names {
            let source = archive.read(name).unwrap();
            let doc = PackfileDocument::parse(&source)
                .unwrap_or_else(|e| panic!("{}:{name}: {e}", path.display()));
            let bytes = doc.to_bytes().unwrap();
            assert_eq!(PackfileDocument::parse(&bytes).unwrap(), doc, "{name}");
            let a = Packfile::parse(&source).unwrap();
            let b = Packfile::parse(&bytes).unwrap();
            assert_eq!(a.data, b.data, "{name}");
            assert_eq!(a.local_fixups, b.local_fixups, "{name}");
            assert_eq!(a.global_fixups, b.global_fixups, "{name}");
            assert_eq!(a.objects, b.objects, "{name}");
            for (class, kind) in [
                ("hkaAnimationContainer", 0),
                ("hkbBehaviorGraph", 1),
                ("hkbCharacterData", 2),
                ("hkbProjectData", 3),
            ] {
                if !a.objects_of(class).is_empty() {
                    match kind {
                        0 => assert_eq!(
                            AnimationContainer::from_packfile(&a).unwrap(),
                            AnimationContainer::from_packfile(&b).unwrap(),
                            "{name}"
                        ),
                        1 => assert_eq!(behavior_model(&a), behavior_model(&b), "{name}"),
                        2 => assert_eq!(
                            CharacterData::from_packfile(&a).unwrap(),
                            CharacterData::from_packfile(&b).unwrap(),
                            "{name}"
                        ),
                        _ => assert_eq!(
                            ProjectData::from_packfile(&a).unwrap(),
                            ProjectData::from_packfile(&b).unwrap(),
                            "{name}"
                        ),
                    }
                    modeled[kind] += 1;
                }
            }
            for obj in a.objects_of("hkaSplineCompressedAnimation") {
                let before = SplineCompressedAnimation::from_obj(obj).unwrap();
                let after = SplineCompressedAnimation::from_obj(b.object(obj.base)).unwrap();
                assert_eq!(before, after, "{name}");
                modeled[4] += 1;
            }
            opaque += a.objects.values().filter(|c| c.starts_with("hkp")).count();
            count += 1;
        }
        if count > 0 {
            println!("{}: {count} packfiles", path.display());
        }
        files += count;
    }
    println!("ANM-08: {files} packfiles, modeled containers/graphs/characters/projects/splines {modeled:?}, {opaque} opaque hkp objects, zero failures");
    assert!(files >= 7699, "base corpus missing");
    assert!(modeled.iter().all(|&n| n > 0), "missing modeled categories");
    assert!(opaque > 0, "no opaque physics objects checked");
}
