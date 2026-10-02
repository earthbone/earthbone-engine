// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

const SEQUENCE: &str = include_str!("../../probe/probes/anm_54.toml");

#[test]
fn anm_54_tie_claims_require_an_original_runtime_capture() {
    let capture = std::env::var_os("SOE_PROBE_CAPTURE")
        .expect("ANM-54 tie claims need an original-runtime capture");
    let data = std::env::var_os("SKYRIM_DATA")
        .expect("ANM-54 capture comparison needs the installed Skyrim data");
    let probe = soe_probe::Probe::parse(SEQUENCE).expect("valid ANM-54 probe");
    let log = std::fs::read_to_string(std::path::PathBuf::from(capture).join("Papyrus.0.log"))
        .expect("original-runtime probe log");
    let observed = soe_probe::parse_log(&log).expect("parse original-runtime observations");
    let replay = soe_probe::run_installed(&probe, std::path::Path::new(&data))
        .expect("replay ANM-54 against installed HKX");
    let comparison = soe_probe::compare(&probe, &replay, &observed)
        .expect("compare capture boundaries and values");
    assert!(comparison.matched, "capture differs from HKX replay: {:?}", comparison.differences);
}
