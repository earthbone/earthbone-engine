// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_commonlib_shim::quick_loot::{
    ApiVersion, Error, Event, EventKind, HandleResult, QuickLootApi,
};

#[test]
fn ui_26_v20_cannot_register_or_request_v21_actions() {
    let mut api = QuickLootApi::default();
    api.init("mod", ApiVersion::V20).unwrap();
    assert!(api.is_ready("mod", ApiVersion::V20));
    assert!(!api.is_ready("mod", ApiVersion::V21));
    assert_eq!(
        api.register_handler("mod", EventKind::InputAction, Box::new(|_| {})),
        Err(Error::Version)
    );
}

#[test]
fn ui_26_cancellation_stops_later_handlers_in_registration_order() {
    let mut api = QuickLootApi::default();
    api.init("first", ApiVersion::V21).unwrap();
    api.init("second", ApiVersion::V21).unwrap();
    let later_called = std::rc::Rc::new(std::cell::Cell::new(false));
    api.register_handler(
        "first",
        EventKind::OpeningLootMenu,
        Box::new(|event| {
            event.result = HandleResult::Stop;
        }),
    )
    .unwrap();
    let marker = later_called.clone();
    api.register_handler(
        "second",
        EventKind::OpeningLootMenu,
        Box::new(move |_| marker.set(true)),
    )
    .unwrap();
    api.dispatch(&mut Event::new(
        EventKind::OpeningLootMenu,
        soe_commonlib_shim::quick_loot::Handle(1),
    ));
    assert!(!later_called.get());
}
