// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_commonlib_shim::events::{EventSink, NotifyControl, QuestStartStopEvent};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::World;
use soe_papyrus::ObjectId;

struct Capture(Rc<RefCell<Vec<QuestStartStopEvent>>>);

impl EventSink<QuestStartStopEvent> for Capture {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn invalid_stage_does_not_emit_or_change_quest_transition_events() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"QUST"), 0x800).sub(Tag(*b"DNAM"), [0; 12]));
    let mut order = LoadOrder::new();
    order.add("Shm23InvalidStage.esp", plugin).unwrap();
    let mut world = World::new(order);
    let seen = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(!world.set_stage(
        GlobalFormId {
            file: 0,
            object: 0x800
        },
        77,
        &mut allocate
    ));
    assert!(seen.borrow().is_empty());
    assert!(
        !world
            .quest(GlobalFormId {
                file: 0,
                object: 0x800
            })
            .unwrap()
            .running
    );
}
