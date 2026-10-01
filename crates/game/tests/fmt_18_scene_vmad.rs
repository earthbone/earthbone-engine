// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

//! SCEN VMAD fixtures follow xEdit's `wbVMADFragmentedSCEN` field order.

use crate::common::installed;

use soe_esm::Tag;
use soe_game::vmad::Vmad;

const SCEN: Tag = Tag::new(b"SCEN");

fn name(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn authored_scene(flags: u8, phases: &[(u8, u32, u8, &str, &str)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&5i16.to_le_bytes());
    bytes.extend_from_slice(&2i16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.push(2);
    bytes.push(flags);
    name(&mut bytes, "SF_TestScene");
    if flags & 1 != 0 {
        bytes.push(7);
        name(&mut bytes, "SF_TestScene");
        name(&mut bytes, "OnBegin");
    }
    if flags & 2 != 0 {
        bytes.push(8);
        name(&mut bytes, "SF_TestScene");
        name(&mut bytes, "OnEnd");
    }
    bytes.extend_from_slice(&(phases.len() as u16).to_le_bytes());
    for (flag, index, marker, script, function) in phases {
        bytes.push(*flag);
        bytes.extend_from_slice(&index.to_le_bytes());
        bytes.push(*marker);
        name(&mut bytes, script);
        name(&mut bytes, function);
    }
    bytes
}

#[test]
fn fmt_18_scene_begin_end_and_phase_fragments_reproduce_authored_bytes() {
    let phases = [
        (1, 0x1234, 9, "SF_TestScene", "OnPhaseStart"),
        (2, 0x1234, 10, "SF_TestScene", "OnPhaseCompletion"),
    ];
    for flags in [0, 1, 2, 3] {
        let bytes = authored_scene(flags, &phases);
        let parsed = Vmad::parse(&bytes, SCEN).unwrap();
        let scene = parsed.scene_fragments.as_ref().unwrap();
        assert_eq!(scene.begin.is_some(), flags & 1 != 0);
        assert_eq!(scene.end.is_some(), flags & 2 != 0);
        assert_eq!(
            scene.begin.as_ref().map(|f| f.marker),
            (flags & 1 != 0).then_some(7)
        );
        assert_eq!(
            scene.end.as_ref().map(|f| f.marker),
            (flags & 2 != 0).then_some(8)
        );
        assert_eq!(scene.phases[0].flag, 1);
        assert_eq!(scene.phases[0].phase_index, 0x1234);
        assert_eq!(scene.phases[0].function, "OnPhaseStart");
        assert_eq!(scene.phases[1].flag, 2);
        assert_eq!(scene.phases[1].marker, 10);
        assert_eq!(scene.phases[1].function, "OnPhaseCompletion");
        assert_eq!(parsed.write(SCEN), bytes, "flags {flags}");
    }
}

#[test]
fn fmt_18_scene_fragment_truncation_and_unknown_flags_return_errors() {
    let bytes = authored_scene(3, &[(1, 2, 4, "SF_TestScene", "OnPhase")]);
    for cut in 7..bytes.len() {
        assert!(Vmad::parse(&bytes[..cut], SCEN).is_err(), "cut {cut}");
    }
    let mut bad_flags = bytes.clone();
    bad_flags[7] = 4;
    assert!(Vmad::parse(&bad_flags, SCEN).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(Vmad::parse(&trailing, SCEN).is_err());
}

#[test]
fn fmt_18_installed_scen_fragments_reproduce_authored_vmad_bytes() {
    let Ok(_data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();

    let mut decoded = 0;
    let mut begin = 0;
    let mut end = 0;
    let mut phase = 0;
    for (id, record) in order.winners_of(SCEN) {
        let Some(bytes) = record.find(Tag::new(b"VMAD")) else {
            continue;
        };
        let parsed = Vmad::parse(bytes, SCEN).unwrap_or_else(|e| panic!("SCEN {id:?}: {e}"));
        let Some(scene) = &parsed.scene_fragments else {
            continue;
        };
        decoded += 1;
        begin += usize::from(scene.begin.is_some());
        end += usize::from(scene.end.is_some());
        phase += scene.phases.len();
        assert_eq!(parsed.write(SCEN), bytes, "SCEN {id:?} VMAD bytes differ");
    }
    eprintln!("SCEN VMAD survey: {decoded} tails, {begin} begin, {end} end, {phase} phase");
    assert!(decoded > 0, "expected installed SCEN VMAD tails");
    assert!(begin > 0, "expected installed SCEN begin fragments");
    assert!(end > 0, "expected installed SCEN end fragments");
    assert!(phase > 0, "expected installed SCEN phase fragments");
}
