// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use soe_commonlib_shim::events::{EventSink, NotifyControl, QuestStartStopEvent};
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest::entry_flags, world::World};
use soe_papyrus::ObjectId;

const QUEST: u32 = 0x800;
const MISSING: soe_esm::GlobalFormId = soe_esm::GlobalFormId { file: 0, object: 0x999 };
const QUEST_ID: soe_esm::GlobalFormId = soe_esm::GlobalFormId { file: 0, object: QUEST };
const T: fn(&[u8; 4]) -> Tag = Tag::new;

struct Capture(Rc<RefCell<Vec<QuestStartStopEvent>>>);
impl EventSink<QuestStartStopEvent> for Capture {
    fn process(&mut self, event: &QuestStartStopEvent) -> NotifyControl {
        self.0.borrow_mut().push(*event);
        NotifyControl::Continue
    }
}

fn quest_world(stage_flags: u8) -> World {
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
            .sub(T(b"INDX"), [2, 0, 0, 0])
            .sub(T(b"QSDT"), [stage_flags]),
    );
    let mut order = LoadOrder::new();
    order.add("Quest.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn missing_and_repeated_transitions_emit_only_successful_state_changes() {
    let mut world = quest_world(0);
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
    assert_eq!(
        *seen.borrow(),
        [
            QuestStartStopEvent { form_id: soe_commonlib_shim::form::FormId(QUEST), started: true, failed: false },
            QuestStartStopEvent { form_id: soe_commonlib_shim::form::FormId(QUEST), started: false, failed: false },
        ]
    );
}

#[test]
fn completed_stage_emits_stop_with_resulting_failed_flag_clear() {
    let mut world = quest_world(entry_flags::COMPLETE_QUEST);
    let seen = Rc::new(RefCell::new(Vec::new()));
    world.add_quest_start_stop_sink(Box::new(Capture(seen.clone())));
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(world.set_stage(QUEST_ID, 2, &mut allocate));
    let quest = world.quest(QUEST_ID).unwrap();
    assert!(quest.completed);
    assert!(!quest.running);
    assert_eq!(
        *seen.borrow(),
        [
            QuestStartStopEvent { form_id: soe_commonlib_shim::form::FormId(QUEST), started: true, failed: false },
            QuestStartStopEvent { form_id: soe_commonlib_shim::form::FormId(QUEST), started: false, failed: false },
        ]
    );
}
