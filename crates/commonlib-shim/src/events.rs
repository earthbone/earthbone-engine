// SPDX-License-Identifier: MIT

//! Event channels and payloads from CommonLibSSE-NG's event headers. The
//! event payloads use shim form ids and handles so an engine resolves them
//! through its live object model.

use crate::form::FormId;
use crate::refr::{ActorHandle, ObjectRefHandle};

/// `RE::BSEventNotifyControl`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyControl {
    Continue,
    Stop,
}

pub trait EventSink<E> {
    fn process(&mut self, event: &E) -> NotifyControl;
}

/// `RE::TESGrabReleaseEvent`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TesGrabReleaseEvent {
    pub reference: ObjectRefHandle,
    pub grabbed: bool,
}

/// `RE::TESHarvestedEvent::ItemHarvested` from `RE/I/ItemHarvested.h`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TesHarvestedEvent {
    pub produce_item: FormId,
    pub harvester: ActorHandle,
}

/// `RE::TESHitEvent::Flag`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HitFlags(pub u8);

impl HitFlags {
    pub const POWER_ATTACK: Self = Self(1 << 0);
    pub const SNEAK_ATTACK: Self = Self(1 << 1);
    pub const BASH_ATTACK: Self = Self(1 << 2);
    pub const HIT_BLOCKED: Self = Self(1 << 3);

    /// Whether the event includes this flag.
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

/// `RE::TESHitEvent`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TesHitEvent {
    pub target: ObjectRefHandle,
    pub cause: ObjectRefHandle,
    pub source: FormId,
    pub projectile: FormId,
    pub flags: HitFlags,
}

/// `RE::TESObjectLoadedEvent`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TesObjectLoadedEvent {
    pub form_id: FormId,
    pub loaded: bool,
}

/// `RE::TESQuestStageEvent`. The executable callback pointer is omitted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TesQuestStageEvent {
    pub quest: FormId,
    pub stage: u16,
    pub item_index: u8,
}

/// Hook sites that cannot be represented as engine events or settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookKind {
    CallSite,
    Branch,
    Vtable,
}

/// Result for a request to install a binary hook.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookError {
    Unsupported(HookKind),
}

/// Rejects binary patch hooks, which target Skyrim executable addresses.
pub const fn request_hook(kind: HookKind) -> Result<(), HookError> {
    Err(HookError::Unsupported(kind))
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
