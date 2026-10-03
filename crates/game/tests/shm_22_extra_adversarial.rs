// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, NotifyControl, TESCellFullyLoadedEvent};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

struct Capture(Rc<RefCell<Vec<TESCellFullyLoadedEvent>>>);

impl EventSink<TESCellFullyLoadedEvent> for Capture {
    fn process(&mut self, event: &TESCellFullyLoadedEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn invalid_cell_attachment_is_rejected_without_a_loaded_event() {
    let game = Game::new(LoadOrder::new());
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .cell_fully_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(game
        .world_mut()
        .attach_streaming_cells(&[id(0xdead)])
        .is_err());
    assert!(seen.borrow().is_empty());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Shm22Cell.esp", plugin).unwrap();
    let game = Game::new(order);
    let seen = Rc::new(RefCell::new(Vec::new()));
    game.world_mut()
        .commonlib_event_sources()
        .cell_fully_loaded()
        .add_sink(Box::new(Capture(seen.clone())));
    assert!(game
        .world_mut()
        .attach_streaming_cells(&[id(0x900)])
        .is_ok());
    assert_eq!(seen.borrow().len(), 1);
}
