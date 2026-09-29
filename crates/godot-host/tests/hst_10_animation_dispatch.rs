// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_godot_host::animation_dispatch::{report_graph_events, AnimationEventCursor};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn hst_10_routes_requests_to_the_matching_actor_only() {
    let events = vec![
        (id(1), "MoveStart".to_string()),
        (id(2), "OpenDoor".to_string()),
        (id(1), "AttackStart".to_string()),
    ];
    let mut cursor = AnimationEventCursor::beginning();

    assert_eq!(
        cursor.take_for(&events, id(1)),
        ["MoveStart", "AttackStart"]
    );
}

#[test]
fn hst_10_delivers_a_request_once_and_skips_prebinding_events() {
    let mut events = vec![(id(1), "OldEvent".to_string())];
    let mut cursor = AnimationEventCursor::at_end(&events);
    events.push((id(1), "PlayIdle".to_string()));

    assert_eq!(cursor.take_for(&events, id(1)), ["PlayIdle"]);
    assert!(cursor.take_for(&events, id(1)).is_empty());
}

#[test]
fn hst_10_reports_each_graph_event_once() {
    let mut completions = vec!["IdleStop".to_string(), "WeaponSwing".to_string()];
    let mut reported = Vec::new();
    report_graph_events(&mut completions, |event| reported.push(event.to_string()));
    report_graph_events(&mut completions, |event| reported.push(event.to_string()));

    assert!(completions.is_empty());
    assert_eq!(reported, ["IdleStop", "WeaponSwing"]);
}
