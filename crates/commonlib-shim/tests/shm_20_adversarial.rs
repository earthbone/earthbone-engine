// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{
    EventSink, NotifyControl, ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned,
    TESCellFullyLoadedEvent,
};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{ActorHandle, ActorValue};

#[test]
fn batch_three_event_sources_dispatch_repeated_payloads_without_mutation() {
    let mut shout = soe_commonlib_shim::events::EventSource::default();
    let mut skill = soe_commonlib_shim::events::EventSource::default();
    let mut souls = soe_commonlib_shim::events::EventSource::default();
    let mut spells = soe_commonlib_shim::events::EventSource::default();
    let mut cells = soe_commonlib_shim::events::EventSource::default();
    let seen = std::rc::Rc::new(std::cell::Cell::new(0));
    struct Shared(std::rc::Rc<std::cell::Cell<usize>>);
    impl<E> EventSink<E> for Shared {
        fn process(&mut self, _: &E) -> NotifyControl {
            self.0.set(self.0.get() + 1);
            NotifyControl::Continue
        }
    }
    shout.add_sink(Box::new(Shared(seen.clone())));
    skill.add_sink(Box::new(Shared(seen.clone())));
    souls.add_sink(Box::new(Shared(seen.clone())));
    spells.add_sink(Box::new(Shared(seen.clone())));
    cells.add_sink(Box::new(Shared(seen.clone())));
    for _ in 0..2 {
        assert_eq!(
            shout.send(&ShoutAttack {
                shout: FormId(0xFFFF_FFFE)
            }),
            1
        );
        assert_eq!(
            skill.send(&SkillIncrease {
                player: ActorHandle(u32::MAX),
                actor_value: ActorValue(u32::MAX)
            }),
            1
        );
        assert_eq!(
            souls.send(&SoulsTrapped {
                trapper: ActorHandle(0),
                target: ActorHandle(u32::MAX)
            }),
            1
        );
        assert_eq!(spells.send(&SpellsLearned { spell: FormId(0) }), 1);
        assert_eq!(
            cells.send(&TESCellFullyLoadedEvent {
                cell: FormId(0xFFFF_FFFF)
            }),
            1
        );
    }
    assert_eq!(seen.get(), 10);
}

#[test]
fn cell_loaded_payload_preserves_its_form_id() {
    let payload = TESCellFullyLoadedEvent {
        cell: FormId(0xFEA0_0ABC),
    };
    assert_eq!(payload.cell, FormId(0xFEA0_0ABC));
}
