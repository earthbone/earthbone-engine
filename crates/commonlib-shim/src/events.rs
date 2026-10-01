// SPDX-License-Identifier: MIT

//! Typed event sources exposed through the source-level CommonLib API.

use crate::form::FormId;
use crate::refr::{ActorHandle, ActorValue, ObjectRefHandle};

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

/// An in-process event source. Sinks run in registration order and delivery
/// stops at the first sink that returns `Stop`.
/// The CommonLib `RE::TESQuestStartStopEvent` payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuestStartStopEvent {
    pub form_id: crate::form::FormId,
    pub started: bool,
    pub failed: bool,
}
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
    pub reason: &'static str,
}

impl UnsupportedHook {
    pub const fn new(name: &'static str) -> Self {
        Self { name, reason: name }
    }
}

pub const UNSUPPORTED_VTABLE_HOOK: UnsupportedHook = UnsupportedHook {
    name: "vtable and call-site hooks target Skyrim machine code, which this engine does not use",
    reason: "vtable and call-site hooks target Skyrim machine code, which this engine does not use",
};

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
