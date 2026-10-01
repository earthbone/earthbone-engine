// SPDX-License-Identifier: MIT

//! `RE::BSTEventSource` and `RE::BSTEventSink`: typed channels with the
//! continue-or-stop control a sink returns.

/// `RE::BSEventNotifyControl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyControl {
    Continue,
    Stop,
}

/// Runtime identity for a form used by an event payload. This is kept
/// opaque to the MIT shim and is not a Skyrim binary FormID.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EventFormId(pub u64);

/// `RE::ActorKill::Event`, using runtime form identities in place of actor
/// pointers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActorKill {
    pub killer: EventFormId,
    pub victim: EventFormId,
}

/// `RE::CriticalHit::Event`, using runtime form identities in place of
/// reference pointers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CriticalHit {
    pub aggressor: EventFormId,
    pub weapon: EventFormId,
    pub sneak_hit: bool,
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
