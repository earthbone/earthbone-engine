// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{
    DeathEvent, EnterBleedoutEvent, EventState, FormDeleteEvent, FurnitureEvent,
    FurnitureEventKind,
};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{ActorHandle, ObjectRefHandle};

struct EmptyState;

impl EventState for EmptyState {
    fn inventory_count(&self, _: FormId, _: FormId) -> Option<i32> { None }
    fn is_dead(&self, _: ActorHandle) -> Option<bool> { None }
    fn is_in_bleedout(&self, _: ActorHandle) -> Option<bool> { None }
    fn is_form_deleted(&self, _: FormId) -> Option<bool> { None }
    fn furniture_event(&self, _: ActorHandle) -> Option<FurnitureEvent> { None }
}

#[test]
fn unavailable_records_remain_unknown_instead_of_becoming_false_state() {
    let state = EmptyState;
    assert_eq!(state.inventory_count(FormId(1), FormId(2)), None);
    assert_eq!(state.is_dead(ActorHandle(3)), None);
    assert_eq!(state.is_in_bleedout(ActorHandle(4)), None);
    assert_eq!(state.is_form_deleted(FormId(5)), None);
    assert_eq!(state.furniture_event(ActorHandle(6)), None);
}

#[test]
fn event_payloads_preserve_absent_and_present_actor_references() {
    let no_killer = DeathEvent { actor_dying: ObjectRefHandle(7), actor_killer: None, dead: true };
    let killer = DeathEvent { actor_dying: ObjectRefHandle(7), actor_killer: Some(ObjectRefHandle(8)), dead: false };
    assert_eq!(no_killer.actor_killer, None);
    assert_eq!(killer.actor_killer, Some(ObjectRefHandle(8)));
    assert_eq!(
        EnterBleedoutEvent { actor: ObjectRefHandle(9) }.actor,
        ObjectRefHandle(9)
    );
    assert_eq!(FormDeleteEvent { form_id: FormId(u32::MAX) }.form_id, FormId(u32::MAX));
    assert_eq!(FurnitureEventKind::Enter as u32, 0);
    assert_eq!(FurnitureEventKind::Exit as u32, 1);
}
