// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::scr_09_actor_queries::{actor_bool, fixture, id};
use soe_papyrus::Value;

#[test]
fn detected_by_rejects_an_extra_argument_without_using_it_as_observer() {
    let mut game = fixture();
    let observer = game.object_of_form(id(0x901));
    let unrelated = game.object_of_form(id(0x902));
    assert!(!actor_bool(
        &mut game,
        id(0x900),
        "IsDetectedBy",
        &[Value::Object(observer), Value::Object(unrelated)],
    ));
    assert!(
        game.vm
            .log
            .contains("IsDetectedBy expects one Actor argument"),
        "CK declares a single akOther Actor argument"
    );
}
