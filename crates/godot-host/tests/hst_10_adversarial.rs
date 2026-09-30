// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
use soe_godot_host::animation_dispatch::AnimationEventCursor;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

#[test]
fn hst_10_same_object_number_in_another_file_is_not_the_bound_actor() {
    let log = vec![
        (id(0, 0x123), "WrongFile".to_owned()),
        (id(1, 0x123), "RightActor".to_owned()),
    ];
    let mut cursor = AnimationEventCursor::beginning();
    assert_eq!(cursor.take_for(&log, id(1, 0x123)), ["RightActor"]);
}

#[test]
fn hst_10_each_actor_cursor_observes_its_own_new_requests_once() {
    let mut log = vec![(id(0, 1), "First".to_owned())];
    let mut first = AnimationEventCursor::at_end(&log);
    let mut second = AnimationEventCursor::at_end(&log);
    log.push((id(0, 1), "ActorOne".to_owned()));
    log.push((id(0, 2), "ActorTwo".to_owned()));

    assert_eq!(first.take_for(&log, id(0, 1)), ["ActorOne"]);
    assert_eq!(second.take_for(&log, id(0, 2)), ["ActorTwo"]);
    assert!(first.take_for(&log, id(0, 1)).is_empty());
    assert!(second.take_for(&log, id(0, 2)).is_empty());
}
