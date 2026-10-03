// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

fn observation() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest.join("../../docs/hst-05-audio-observation.md"))
        .expect("HST-05 observation record exists")
}

fn field_value<'a>(record: &'a str, name: &str) -> &'a str {
    record
        .lines()
        .find_map(|line| line.strip_prefix(&format!("- {name}:")))
        .unwrap_or("")
        .trim()
}

#[test]
fn hst_05_observation_records_the_windowed_run() {
    let record = observation();
    for field in ["Date", "Device", "Sound"] {
        assert!(
            !field_value(&record, field).is_empty(),
            "HST-05 physical run is missing {field}"
        );
    }
}

#[test]
fn hst_05_observation_confirms_a_person_heard_the_sound() {
    let record = observation();
    let heard = field_value(&record, "What the person heard");
    assert!(
        !heard.is_empty(),
        "HST-05 has no human listening observation"
    );
    assert!(
        heard.to_ascii_lowercase().contains("audible")
            || heard.to_ascii_lowercase().contains("heard"),
        "HST-05 listening observation does not say the sound was heard: {heard}"
    );
}

#[test]
fn hst_05_observation_records_successful_process_exit() {
    let record = observation();
    let status = field_value(&record, "Process exit status");
    assert_eq!(
        status, "0",
        "HST-05 process exit status was not recorded as success"
    );
}
