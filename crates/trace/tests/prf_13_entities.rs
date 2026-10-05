// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-13 scope accounting and export contract from docs/performance.md.

use soe_trace::{count, Entity, EntityScope, Operation, Profile};
use std::sync::Mutex;
use std::time::Duration;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_disabled_factories_and_counts_do_not_record() {
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    let scope = EntityScope::new(|| panic!("disabled scope constructed metadata"), 1);
    assert!(Profile::input(|| panic!("disabled input constructed metadata")).is_none());
    Profile::wait(|| panic!("disabled wait constructed metadata"), 1.0, 1);
    count(Operation::NativeCalls, 1);
    drop(scope);
    assert!(Profile::summary().is_empty());
}

#[test]
fn prf_13_nested_entities_charge_plugins_once_and_export_perfetto_events() {
    let _guard = LOCK.lock().unwrap();
    Profile::clear();
    Profile::enable();
    let outer = Entity::new("actor", "actor_1", "Actor.esm", "planning");
    let inner = Entity::new("script", "QuestScript.Run", "Patch.esp", "execute");
    {
        let _actor = EntityScope::new(|| outer.clone(), 1);
        std::thread::sleep(Duration::from_millis(1));
        {
            let _script = EntityScope::new(|| inner.clone(), 1);
            count(Operation::PapyrusInstructions, 3);
            count(Operation::NativeCalls, 2);
            std::thread::sleep(Duration::from_millis(1));
        }
        count(Operation::Conditions, 1);
    }
    Profile::wait(|| inner.clone(), 0.25, 1);
    let input = Profile::input(|| "key_W".to_owned()).unwrap();
    input.presented();
    for _ in 0..4 {
        Profile::render_submission(|| ());
    }
    Profile::disable();
    let rows = Profile::summary();
    assert_eq!(
        rows[&outer].total_ns,
        rows[&outer].self_ns + rows[&inner].total_ns
    );
    assert_eq!(rows[&inner].work.get(Operation::PapyrusInstructions), 3);
    assert_eq!(rows[&inner].work.get(Operation::NativeCalls), 2);
    assert_eq!(rows[&inner].wait_ns, 250_000_000);
    assert_eq!(rows[&inner].waits, 1);
    assert_eq!(
        Profile::ranking(&rows, "plugin")
            .iter()
            .map(|(_, row)| row.self_ns)
            .sum::<u64>(),
        rows.values().map(|row| row.self_ns).sum::<u64>()
    );
    assert!(Profile::overlay_text().contains("QuestScript"));
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("soe-prf-13-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let trace = directory.join("trace.json");
    let summary = directory.join("summary.json");
    Profile::export_chrome_json(&trace).unwrap();
    Profile::export_summary(&summary).unwrap();
    let trace: serde_json::Value = serde_json::from_slice(&std::fs::read(trace).unwrap()).unwrap();
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(summary).unwrap()).unwrap();
    assert!(trace["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["ph"] == "X" && event["args"]["plugin"].is_string()));
    assert!(summary["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["cat"] == "plugin"
            && event["name"] == "Patch.esp"
            && event["args"]["papyrus_instructions"] == 3));
    assert!(Profile::export_summary(&directory.join("absent/summary.json")).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn prf_13_rolling_window_expires_without_losing_session_totals() {
    let _guard = LOCK.lock().unwrap();
    Profile::clear();
    Profile::enable();
    {
        let _entity = EntityScope::new(|| Entity::new("quest", "old", "Base.esm", "stage"), 1);
    }
    Profile::disable();
    std::thread::sleep(Duration::from_millis(1010));
    assert!(Profile::last_second().is_empty());
    assert_eq!(Profile::summary().len(), 1);
}
