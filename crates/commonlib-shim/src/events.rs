// SPDX-License-Identifier: MIT

//! Typed event sources exposed through the source-level CommonLib API.

/// `RE::BSEventNotifyControl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyControl {
    Continue,
    Stop,
}

pub trait EventSink<E> {
    fn process(&mut self, event: &E) -> NotifyControl;
}

/// An in-process event source. Sinks run in registration order.
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

/// A stable load-order identity represented without depending on the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormIdentity(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisarmedEvent {
    pub source: FormIdentity,
    pub target: FormIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonSoulsGained {
    pub souls: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelIncrease {
    pub player: FormIdentity,
    pub new_level: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationDiscovery {
    pub map_marker: FormIdentity,
    pub worldspace_id: Option<String>,
}

/// Live event channels corresponding to the CommonLib source names in this
/// API batch. The engine owns this value and emits events into its channels.
#[derive(Default)]
pub struct ScriptEventSourceHolder {
    disarmed: EventSource<DisarmedEvent>,
    dragon_souls_gained: EventSource<DragonSoulsGained>,
    level_increase: EventSource<LevelIncrease>,
    location_discovery: EventSource<LocationDiscovery>,
}

impl ScriptEventSourceHolder {
    pub fn disarmed(&mut self) -> &mut EventSource<DisarmedEvent> {
        &mut self.disarmed
    }

    pub fn dragon_souls_gained(&mut self) -> &mut EventSource<DragonSoulsGained> {
        &mut self.dragon_souls_gained
    }

    pub fn level_increase(&mut self) -> &mut EventSource<LevelIncrease> {
        &mut self.level_increase
    }

    pub fn location_discovery(&mut self) -> &mut EventSource<LocationDiscovery> {
        &mut self.location_discovery
    }
}

/// A hook that requires patching executable code cannot be installed through
/// the source-level compatibility API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedHook {
    pub name: &'static str,
}

impl UnsupportedHook {
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }
}

pub fn install_hook(hook: UnsupportedHook) -> Result<(), UnsupportedHook> {
    Err(hook)
}

/// The story and script event names subscribed to by the surveyed plugins.
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
    impl<E> EventSink<E> for Counter {
        fn process(&mut self, _: &E) -> NotifyControl {
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
