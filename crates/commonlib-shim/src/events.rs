// SPDX-License-Identifier: MIT

//! `RE::BSTEventSource` and `RE::BSTEventSink`: typed channels with the
//! continue-or-stop control a sink returns.

use crate::form::FormId;
use crate::refr::{ActorHandle, ObjectRefHandle};

/// Inventory transfer data delivered after the engine changes a container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContainerChangedEvent {
    pub old_container: FormId,
    pub new_container: FormId,
    pub base_object: FormId,
    pub item_count: i32,
    pub reference: ObjectRefHandle,
    pub unique_id: u16,
}

/// An actor death transition and its optional killer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathEvent {
    pub actor_dying: ObjectRefHandle,
    pub actor_killer: Option<ObjectRefHandle>,
    pub dead: bool,
}

/// An actor entered bleedout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnterBleedoutEvent {
    pub actor: ObjectRefHandle,
}

/// A form was removed from live use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormDeleteEvent {
    pub form_id: FormId,
}

/// An actor entered or left a furniture reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureEvent {
    pub actor: ObjectRefHandle,
    pub target_furniture: ObjectRefHandle,
    pub kind: FurnitureEventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FurnitureEventKind {
    Enter = 0,
    Exit = 1,
}

/// Hooks that depend on Skyrim executable addresses have no target here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHook(pub &'static str);

pub const UNSUPPORTED_VTABLE_HOOK: UnsupportedHook = UnsupportedHook(
    "vtable and call-site hooks target Skyrim machine code, which this engine does not use",
);

/// `RE::BSEventNotifyControl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyControl {
    Continue,
    Stop,
}

pub trait EventSink<E> {
    fn process(&mut self, event: &E) -> NotifyControl;
}

/// A source owns its sinks and delivers in registration order, stopping
/// at the first sink that says so, as the game's does.
pub struct EventSource<E> {
    sinks: Vec<Box<dyn EventSink<E>>>,
}

impl<E> Default for EventSource<E> {
    fn default() -> Self {
        Self { sinks: Vec::new() }
    }
}

impl<E> EventSource<E> {
    pub fn add_sink(&mut self, sink: Box<dyn EventSink<E>>) {
        self.sinks.push(sink);
    }

    /// Returns how many sinks saw the event.
    pub fn send(&mut self, event: &E) -> usize {
        let mut seen = 0;
        for sink in &mut self.sinks {
            seen += 1;
            if sink.process(event) == NotifyControl::Stop {
                break;
            }
        }
        seen
    }
}

/// The story and script events the sample plugins subscribe to, one
/// variant per `RE::TES*Event` in `docs/shim.md`. Payloads are filled in
/// when the `game` crate defines them, the names are fixed now so a
/// plugin's sink list can be written against them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScriptEvent {
    CellFullyLoaded,
    ContainerChanged,
    Death,
    EnterBleedout,
    FormDelete,
    Furniture,
    GrabRelease,
    Harvested,
    Hit,
    ObjectLoaded,
    QuestStage,
    QuestStartStop,
    Disarmed,
    MenuOpenClose,
    CrosshairRef,
    ActorKill,
    CriticalHit,
    DragonSoulsGained,
    LevelIncrease,
    LocationDiscovery,
    ShoutAttack,
    SkillIncrease,
    SoulsTrapped,
    SpellsLearned,
}

/// The live state operations event adapters may request from the engine.
/// Implementations must read current state rather than maintain a parallel copy.
pub trait EventState {
    fn inventory_count(&self, container: FormId, item: FormId) -> Option<i32>;
    fn is_dead(&self, actor: ActorHandle) -> Option<bool>;
    fn is_in_bleedout(&self, actor: ActorHandle) -> Option<bool>;
    fn is_form_deleted(&self, form: FormId) -> Option<bool>;
    fn furniture_event(&self, actor: ActorHandle) -> Option<FurnitureEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Counter(Rc<Cell<u32>>, NotifyControl);
    impl EventSink<ScriptEvent> for Counter {
        fn process(&mut self, _: &ScriptEvent) -> NotifyControl {
            self.0.set(self.0.get() + 1);
            self.1
        }
    }

    #[test]
    fn a_stopping_sink_ends_delivery() {
        let n = Rc::new(Cell::new(0));
        let mut src = EventSource::default();
        src.add_sink(Box::new(Counter(n.clone(), NotifyControl::Continue)));
        src.add_sink(Box::new(Counter(n.clone(), NotifyControl::Stop)));
        src.add_sink(Box::new(Counter(n.clone(), NotifyControl::Continue)));
        assert_eq!(src.send(&ScriptEvent::Death), 2);
        assert_eq!(n.get(), 2);
    }
}
