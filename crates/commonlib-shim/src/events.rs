// SPDX-License-Identifier: MIT

//! `RE::BSTEventSource` and `RE::BSTEventSink`: typed channels with the
//! continue-or-stop control a sink returns.

use crate::form::FormId;
use crate::refr::{ActorHandle, ActorValue};

/// `RE::BSEventNotifyControl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyControl {
    Continue,
    Stop,
}

pub trait EventSink<E> {
    fn process(&mut self, event: &E) -> NotifyControl;
}

/// `RE::ShoutAttack::Event`, identifying the shout form used by the actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShoutAttack {
    pub shout: FormId,
}

/// `RE::SkillIncrease::Event`, identifying the player and advanced skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillIncrease {
    pub player: ActorHandle,
    pub actor_value: ActorValue,
}

/// `RE::SoulsTrapped::Event`, identifying the actor and its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoulsTrapped {
    pub trapper: ActorHandle,
    pub target: ActorHandle,
}

/// `RE::SpellsLearned::Event`, identifying the spell form learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellsLearned {
    pub spell: FormId,
}

/// `RE::TESCellFullyLoadedEvent`, identifying the cell that finished loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TESCellFullyLoadedEvent {
    pub cell: FormId,
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
