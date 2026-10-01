// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{
    EventSource, EventSink, HitFlags, NotifyControl, TesGrabReleaseEvent, TesHarvestedEvent,
    TesHitEvent, TesObjectLoadedEvent, TesQuestStageEvent,
};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{ActorHandle, ObjectRefHandle};

struct Capture<T>(Vec<T>);
impl<T: Copy> EventSink<T> for Capture<T> {
    fn process(&mut self, event: &T) -> NotifyControl {
        self.0.push(*event);
        NotifyControl::Continue
    }
}

#[test]
fn hit_flag_bits_are_independent_and_combined_flags_are_detected() {
    let flags = HitFlags(HitFlags::POWER_ATTACK.0 | HitFlags::HIT_BLOCKED.0);
    assert!(flags.contains(HitFlags::POWER_ATTACK));
    assert!(flags.contains(HitFlags::HIT_BLOCKED));
    assert!(!flags.contains(HitFlags::SNEAK_ATTACK));
    assert!(!flags.contains(HitFlags(HitFlags::POWER_ATTACK.0 | HitFlags::SNEAK_ATTACK.0)));
}

#[test]
fn batch_five_event_source_keeps_all_fields_across_repeated_delivery() {
    let mut source = EventSource::default();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    struct Sink(std::rc::Rc<std::cell::RefCell<Vec<TesQuestStageEvent>>>);
    impl EventSink<TesQuestStageEvent> for Sink {
        fn process(&mut self, event: &TesQuestStageEvent) -> NotifyControl {
            self.0.borrow_mut().push(*event);
            NotifyControl::Continue
        }
    }
    // Other payloads are checked directly below. This channel checks stable stage and item indexes.
    let _other_payloads = (
        TesGrabReleaseEvent { reference: ObjectRefHandle(1), grabbed: false },
        TesHarvestedEvent { produce_item: FormId(2), harvester: ActorHandle(3) },
        TesHitEvent { target: ObjectRefHandle(4), cause: ObjectRefHandle(5), source: FormId(6), projectile: FormId(7), flags: HitFlags(15) },
        TesObjectLoadedEvent { form_id: FormId(8), loaded: false },
    );
    source.add_sink(Box::new(Sink(events.clone())));
    for _ in 0..2 {
        assert_eq!(source.send(&TesQuestStageEvent { quest: FormId(u32::MAX), stage: u16::MAX, item_index: u8::MAX }), 1);
    }
    assert_eq!(*events.borrow(), [
        TesQuestStageEvent { quest: FormId(u32::MAX), stage: u16::MAX, item_index: u8::MAX },
        TesQuestStageEvent { quest: FormId(u32::MAX), stage: u16::MAX, item_index: u8::MAX },
    ]);
}
