// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Handle, Pending};

fn fixture() -> (Game, GlobalFormId) {
    let mut plugin = Plugin::new(FileHeader::default());
    let reference = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    plugin.push_top(Record::new(Tag::new(b"REFR"), reference.object));
    let mut order = LoadOrder::new();
    order.add("Empty.esp", plugin).unwrap();
    (Game::new(order), reference)
}

#[test]
fn prf_19_pending_event_without_a_handler_still_counts_as_queued() {
    let (mut game, _) = fixture();
    let object = game.vm.create_object("object");
    game.world_mut().pending.push(Pending::Event {
        object,
        name: "OnItemAdded".into(),
        args: vec![],
    });

    assert_eq!(game.vm.event_scripts(object, "OnItemAdded").len(), 0);
    assert_eq!(game.papyrus_health().total.events["onitemadded"].queued, 1);
}

#[test]
fn prf_19_reference_event_without_a_handler_still_counts_as_queued() {
    let (mut game, reference) = fixture();
    let reference_object = game.object_of(Handle::Form(reference));
    game.world_mut().pending.push(Pending::Event {
        object: reference_object,
        name: "OnMissingHandler".into(),
        args: vec![],
    });
    assert_eq!(
        game.vm
            .event_scripts(reference_object, "OnMissingHandler")
            .len(),
        0
    );
    assert_eq!(
        game.papyrus_health().total.events["onmissinghandler"].queued,
        1
    );
}
