// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Compare a saved Skyrim capture with the engine trace for PRG-21.

use soe_probe::{compare, parse_log, run_installed, Observation, Probe};

const PROBE: &str = include_str!("../probes/prg_21.toml");
const EXPECTED: &str = include_str!("../probes/prg_21.expected.json");

#[test]
fn prg_21_expected_engine_trace_covers_all_authored_case_labels() {
    let probe = Probe::parse(PROBE).unwrap();
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    let ids: Vec<_> = probe
        .steps
        .iter()
        .map(|step| {
            serde_json::to_value(step).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(expected.len(), ids.len());
    assert!(expected
        .iter()
        .zip(ids)
        .all(|(observation, id)| observation.request_id == id));
    assert!(compare(&probe, &expected, &expected).unwrap().matched);
}

#[test]
fn prg_21_saved_capture_matches_engine_trace_when_available() {
    let Ok(capture_dir) = std::env::var("SOE_PROBE_CAPTURE") else {
        eprintln!("SOE_PROBE_CAPTURE not set, skipping saved Skyrim capture comparison");
        return;
    };
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        panic!("SKYRIM_DATA is required when SOE_PROBE_CAPTURE is set");
    };

    let probe = Probe::parse(PROBE).unwrap();
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    let engine = run_installed(&probe, std::path::Path::new(&data)).unwrap();
    assert!(compare(&probe, &engine, &expected).unwrap().matched);

    let log =
        std::fs::read_to_string(std::path::Path::new(&capture_dir).join("Papyrus.0.log")).unwrap();
    let observed = parse_log(&log).unwrap();
    let report = compare(&probe, &engine, &observed).unwrap();
    assert!(report.matched, "capture differs from engine: {report:?}");
}
