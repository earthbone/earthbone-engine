// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_trace::{Entity, EntityScope, Profile};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_plugin_export_aggregates_nested_entities_without_double_counting_self_time() {
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    Profile::enable();
    let plugin = "Patch\"Quoted.esp";
    let actor = Entity::new("actor", "actor-1", plugin, "planning");
    let script = Entity::new("script", "QuestScript.Run", plugin, "execute");
    {
        let _actor = EntityScope::new(|| actor.clone(), 1);
        {
            let _script = EntityScope::new(|| script.clone(), 1);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    Profile::disable();

    let entities = Profile::summary();
    let expected_self: u64 = entities.values().map(|stats| stats.self_ns).sum();
    let expected_total: u64 = entities.values().map(|stats| stats.total_ns).sum();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("summary.json");
    Profile::export_summary(&path).unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let plugin_row = summary["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["cat"] == "plugin" && event["name"] == plugin)
        .unwrap();
    assert_eq!(plugin_row["args"]["self_ns"], expected_self);
    assert_eq!(plugin_row["args"]["total_ns"], expected_total);
    assert_eq!(plugin_row["args"]["calls"], 2);
}
