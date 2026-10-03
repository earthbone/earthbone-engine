// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, NotifyControl, QuestStartStopEvent};
use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest::entry_flags, world::World};
use soe_papyrus::ObjectId;

const QUEST: u32 = 0x800;
const QUEST_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: QUEST,
};
const MISSING: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x999,
};

struct Capture(Rc<RefCell<Vec<QuestStartStopEvent>>>);

impl EventSink<QuestStartStopEvent> for Capture {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

fn quest_world(stage_flags: Option<u8>) -> World {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    let mut quest = Record::new(Tag(*b"QUST"), QUEST).sub(Tag(*b"DNAM"), [0; 12]);
    if let Some(stage_flags) = stage_flags {
        quest = quest
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0, 0, 0, 0])
            .sub(Tag(*b"INDX"), [2, 0, 0, 0])
            .sub(Tag(*b"QSDT"), [stage_flags]);
    }
    plugin.push_top(quest);
    let mut order = LoadOrder::new();
    order.add("Quest.esm", plugin).unwrap();
    World::new(order)
}

fn event(started: bool, failed: bool) -> QuestStartStopEvent {
    QuestStartStopEvent {
        form_id: soe_commonlib_shim::form::FormId(QUEST),
        started,
        failed,
    }
}

#[test]
fn missing_and_repeated_transitions_emit_only_successful_changes() {
    let mut world = quest_world(None);
    let seen = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(!world.start_quest(MISSING, &mut allocate));
    assert!(!world.stop_quest(MISSING));
    assert!(seen.borrow().is_empty());

    assert!(world.start_quest(QUEST_ID, &mut allocate));
    assert!(!world.start_quest(QUEST_ID, &mut allocate));
    assert!(world.stop_quest(QUEST_ID));
    assert!(!world.stop_quest(QUEST_ID));
    assert_eq!(*seen.borrow(), [event(true, false), event(false, false)]);
}

#[test]
fn completing_stage_reports_started_then_stopped_with_clear_failure_flag() {
    let mut world = quest_world(Some(entry_flags::COMPLETE_QUEST));
    let seen = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(world.set_stage(QUEST_ID, 2, &mut allocate));
    let quest = world.quest(QUEST_ID).unwrap();
    assert!(quest.completed);
    assert!(!quest.running);
    assert_eq!(*seen.borrow(), [event(true, false), event(false, false)]);
}

#[test]
fn failing_stage_reports_stopped_with_resulting_failure_flag() {
    let mut world = quest_world(Some(entry_flags::FAIL_QUEST));
    let seen = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(world.set_stage(QUEST_ID, 2, &mut allocate));
    let quest = world.quest(QUEST_ID).unwrap();
    assert!(quest.failed);
    assert!(!quest.running);
    assert_eq!(*seen.borrow(), [event(true, true), event(false, true)]);
}
