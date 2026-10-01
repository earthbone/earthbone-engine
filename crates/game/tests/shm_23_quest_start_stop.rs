// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, NotifyControl, QuestStartStopEvent};
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest::entry_flags, world::World};
use soe_papyrus::ObjectId;

const QUEST: u32 = 0x800;
const QUEST_ID: soe_esm::GlobalFormId = soe_esm::GlobalFormId {
    file: 0,
    object: QUEST,
};
const T: fn(&[u8; 4]) -> Tag = Tag::new;

struct Capture(Rc<RefCell<Vec<QuestStartStopEvent>>>);

impl EventSink<QuestStartStopEvent> for Capture {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn shm_23_live_quest_transitions_reach_the_mit_event_sink() {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0])
            .sub(T(b"INDX"), [1, 0, 0, 0])
            .sub(T(b"QSDT"), [entry_flags::FAIL_QUEST]),
    );
    let mut order = LoadOrder::new();
    order.add("Quest.esm", plugin).unwrap();
    let mut world = World::new(order);
    let events = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(events.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(world.start_quest(QUEST_ID, &mut allocate));
    assert!(world.quest(QUEST_ID).unwrap().running);
    assert!(world.set_stage(QUEST_ID, 1, &mut allocate));
    let state = world.quest(QUEST_ID).unwrap();
    assert!(!state.running);
    assert!(state.failed);

    assert_eq!(
        *events.borrow(),
        vec![
            QuestStartStopEvent {
                form_id: soe_commonlib_shim::form::FormId(0x0000_0800),
                started: true,
                failed: false,
            },
            QuestStartStopEvent {
                form_id: soe_commonlib_shim::form::FormId(0x0000_0800),
                started: false,
                failed: true,
            },
        ]
    );
}
