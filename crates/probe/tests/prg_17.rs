// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use soe_probe::{compare, parse_log, run_installed, Probe};

const SEQUENCE: &str = include_str!("../probes/prg_17.toml");

#[test]
fn prg_17_installed_perk_forms_replay() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let probe = Probe::parse(SEQUENCE).unwrap();
    let skyrim =
        soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap())
            .unwrap();
    let rank_one = skyrim
        .records
        .iter()
        .find(|record| record.form_id == 0x0babe4)
        .unwrap();
    let rank_two = skyrim
        .records
        .iter()
        .find(|record| record.form_id == 0x079343)
        .unwrap();
    assert_eq!(
        rank_one
            .subrecords
            .iter()
            .find(|subrecord| subrecord.tag == soe_esm::Tag(*b"EDID"))
            .unwrap()
            .data,
        b"Armsman00\0"
    );
    assert_eq!(
        rank_one
            .subrecords
            .iter()
            .find(|subrecord| subrecord.tag == soe_esm::Tag(*b"NNAM"))
            .unwrap()
            .data,
        0x079343u32.to_le_bytes()
    );
    assert_eq!(
        rank_two
            .subrecords
            .iter()
            .find(|subrecord| subrecord.tag == soe_esm::Tag(*b"EDID"))
            .unwrap()
            .data,
        b"Armsman20\0"
    );
    let observations = run_installed(&probe, Path::new(&data)).unwrap();
    assert_eq!(observations.len(), 13);
    for index in [2, 3, 8, 9, 11, 12] {
        assert_eq!(observations[index].result["value"], 1.0);
    }
}

#[test]
fn prg_17_sequence_compiles_acquisition_and_readback_cases() {
    let probe = Probe::parse(SEQUENCE).unwrap();
    assert_eq!(probe.steps.len(), 13);
    for id in [
        "acquire_rank_one",
        "acquire_rank_two",
        "forward_first_owned",
        "forward_second_owned",
        "remove_rank_one",
        "remove_rank_two",
        "acquire_rank_two_first",
        "acquire_rank_one_second",
        "reverse_first_owned",
        "reverse_second_owned",
        "request_save",
        "saved_first_owned",
        "saved_second_owned",
    ] {
        assert!(SEQUENCE.contains(&format!("id = \"{id}\"")));
    }
    let pex = soe_probe::script(&probe).unwrap();
    let listing = format!("{pex:?}");
    assert!(listing.contains("AddPerk"));
    assert!(listing.contains("RemovePerk"));
    assert!(listing.contains("HasPerk"));
    assert!(listing.contains("RequestSave"));
}

#[test]
fn prg_17_vanilla_capture_matches_engine_replay() {
    let Some(capture_dir) = std::env::var_os("SOE_PROBE_CAPTURE") else {
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        panic!("SOE_PROBE_CAPTURE requires SKYRIM_DATA for the engine replay");
    };
    let log = std::fs::read_to_string(Path::new(&capture_dir).join("Papyrus.0.log")).unwrap();
    let observed = parse_log(&log).unwrap();
    let probe = Probe::parse(SEQUENCE).unwrap();
    let expected = run_installed(&probe, Path::new(&data)).unwrap();
    let report = compare(&probe, &expected, &observed).unwrap();
    assert!(report.matched, "{:?}", report.differences);
}
