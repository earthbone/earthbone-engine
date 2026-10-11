// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_game::world::AnimationRequest;
use soe_godot_host::animation_dispatch::AnimationEventCursor;

#[test]
fn hst_10_reference_request_keeps_full_form_identity_and_is_taken_once() {
    let reference = GlobalFormId {
        file: 2,
        object: 0x1234,
    };
    let requests = vec![AnimationRequest {
        target: reference,
        partner: None,
        animation: "Open".to_owned(),
    }];
    let mut cursor = AnimationEventCursor::beginning();

    assert_eq!(cursor.take_for(&requests, reference), ["Open"]);
    assert!(cursor.take_for(&requests, reference).is_empty());
}

#[test]
fn hst_10_same_object_number_on_actor_does_not_match_reference_request() {
    let reference = GlobalFormId {
        file: 2,
        object: 0x1234,
    };
    let actor = GlobalFormId {
        file: 3,
        object: 0x1234,
    };
    let requests = vec![AnimationRequest {
        target: reference,
        partner: None,
        animation: "Open".to_owned(),
    }];
    let mut actor_cursor = AnimationEventCursor::beginning();
    let mut reference_cursor = AnimationEventCursor::beginning();

    assert!(actor_cursor.take_for(&requests, actor).is_empty());
    assert_eq!(reference_cursor.take_for(&requests, reference), ["Open"]);
    assert!(actor_cursor.take_for(&requests, actor).is_empty());
}
