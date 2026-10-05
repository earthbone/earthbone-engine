// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-22 adds stable identities and separate system domains to PRF-13 exports.

use soe_trace::{Entity, EntityScope, FrameSample, Profile, System, SYSTEM_COUNT};

#[test]
fn prf_22_summary_preserves_metadata_and_excludes_overlapping_systems_from_plugins() {
    Profile::disable();
    Profile::clear();
    let frame = FrameSample {
        frame_ns: 100,
        system_ns: [10; SYSTEM_COUNT],
    };
    Profile::systems("game", &frame);
    assert!(Profile::summary().is_empty());
    Profile::enable();
    let actor = Entity::new("actor", "actor.with.dots", "Base.esm", "planning");
    {
        let _scope = EntityScope::new(|| actor.clone(), 1);
    }
    Profile::systems("game", &frame);
    Profile::systems("host", &frame);
    Profile::disable();
    let totals = Profile::summary();
    assert_eq!(
        Profile::ranking(&totals, "plugin"),
        vec![("Base.esm".into(), totals[&actor].clone())]
    );
    assert_eq!(
        totals[&Entity::new("system", System::Papyrus.name(), "", "game")].self_ns,
        10
    );
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("prf-22-summary-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("summary.json");
    Profile::export_summary(&path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let events = value["traceEvents"].as_array().unwrap();
    let event = events.iter().find(|event| event["cat"] == "actor").unwrap();
    assert_eq!(event["args"]["entity"], "actor.with.dots");
    assert_eq!(event["args"]["plugin"], "Base.esm");
    assert_eq!(event["args"]["phase"], "planning");
    assert_eq!(
        events
            .iter()
            .filter(|event| event["cat"] == "system")
            .count(),
        SYSTEM_COUNT * 2
    );
    std::fs::remove_dir_all(directory).unwrap();
}
