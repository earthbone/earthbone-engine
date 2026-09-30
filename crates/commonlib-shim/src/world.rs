// SPDX-License-Identifier: MIT

//! Narrow views of actor, reference, AI and active-effect state, based on
//! CommonLibSSE-NG's `Actor.h`, `ActorValues.h`, `AIProcess.h`,
//! `ExtraMissingRefIDs.h` and `ActiveEffect.h` declarations.

use crate::form::FormId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
/// CommonLib actor critical-stage values supported by the runtime.
pub enum ActorCriticalStage {
    None = 0,
    GooStart = 1,
    GooEnd = 2,
    DisintegrateStart = 3,
    DisintegrateEnd = 4,
}

impl TryFrom<u32> for ActorCriticalStage {
    type Error = UnsupportedState;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::GooStart),
            2 => Ok(Self::GooEnd),
            3 => Ok(Self::DisintegrateStart),
            4 => Ok(Self::DisintegrateEnd),
            _ => Err(UnsupportedState::InvalidCriticalStage(value)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
/// CommonLib actor-value modifier component identifiers.
pub enum ActorValueModifier {
    Permanent = 0,
    Temporary = 1,
    Damage = 2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// An authored activation-parent reference and its delay.
pub struct ActivateParentID {
    pub ref_id: FormId,
    pub activate_delay: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// The supported live package and procedure view of CommonLib `AIProcess`.
pub struct AIProcess {
    pub running_package: Option<FormId>,
    pub procedure: String,
}

#[derive(Clone, Debug, PartialEq)]
/// Live values exposed for CommonLib `ActiveEffect` applications.
pub struct ActiveEffect {
    pub id: u64,
    pub spell: FormId,
    pub effect: FormId,
    pub source: FormId,
    pub caster: Option<FormId>,
    pub target: FormId,
    pub magnitude: f32,
    pub duration: Option<f64>,
    pub elapsed: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A missing world object, invalid value, or engine hook the shim cannot bind.
pub enum UnsupportedState {
    InvalidCriticalStage(u32),
    MissingActor,
    MissingReference,
    MissingActiveEffect,
    Hook(&'static str),
}

impl std::fmt::Display for UnsupportedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCriticalStage(value) => {
                write!(f, "unsupported actor critical stage {value}")
            }
            Self::MissingActor => f.write_str("actor state is unavailable"),
            Self::MissingReference => f.write_str("reference state is unavailable"),
            Self::MissingActiveEffect => f.write_str("active effect state is unavailable"),
            Self::Hook(name) => write!(f, "unsupported CommonLib hook: {name}"),
        }
    }
}

impl std::error::Error for UnsupportedState {}

/// Runtime-backed access implemented by the game crate.
pub trait WorldState {
    /// Read the live critical stage of an actor.
    fn critical_stage(&mut self, actor: FormId) -> Result<ActorCriticalStage, UnsupportedState>;
    /// Set the live critical stage of an actor.
    fn set_critical_stage(
        &mut self,
        actor: FormId,
        stage: ActorCriticalStage,
    ) -> Result<(), UnsupportedState>;
    /// Read one live actor-value component.
    fn actor_value_modifier(
        &mut self,
        actor: FormId,
        actor_value: u32,
        modifier: ActorValueModifier,
    ) -> Result<f32, UnsupportedState>;
    /// Read the package and procedure exposed by the game AI driver.
    fn ai_process(&mut self, actor: FormId) -> Result<AIProcess, UnsupportedState>;
    /// Read all authored XAPR parent references from a placed reference.
    fn activate_parents(
        &mut self,
        reference: FormId,
    ) -> Result<Vec<ActivateParentID>, UnsupportedState>;
    /// Read one live active effect by application identity.
    fn active_effect(&self, id: u64) -> Result<ActiveEffect, UnsupportedState>;
    /// Return an explicit unsupported-hook error for an unavailable runtime hook.
    fn unsupported_hook(&self, name: &'static str) -> Result<(), UnsupportedState>;
}

/// An explicit rejection for a `AIProcess` operation that depends on Skyrim
/// internals with no engine equivalent.
pub fn reject_ai_process_hook(name: &'static str) -> Result<(), UnsupportedState> {
    Err(UnsupportedState::Hook(name))
}

// Narrow runtime-facing contracts for CommonLib world singletons.

/// Runtime operations whose source-level names have no implementation in the
/// engine. Callers receive this error instead of a silent default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedHook {
    /// Projectile vtable and impact callbacks are not implemented.
    ArrowProjectile,
    /// Skyrim's binary load-form buffer layout is not implemented.
    LoadFormBuffer,
}

/// State supplied by the running engine to the MIT compatibility surface.
/// Implementations must read and update the engine's authoritative state.
pub trait ActorValueWorldState {
    fn actor_value(&self, actor: u32, value: ActorValue) -> Option<f32>;
    fn set_actor_value(&mut self, actor: u32, value: ActorValue, amount: f32) -> bool;
    fn actor_value_name(&self, value: ActorValue) -> Option<String>;
    fn actor_value_by_name(&self, name: &str) -> Option<ActorValue>;
    fn default_object(&self, key: [u8; 4]) -> Option<u32>;
}

/// CommonLib actor-value identifier, using the vanilla record index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActorValue(pub u32);

impl ActorValue {
    pub const HEALTH: Self = Self(24);
    pub const MAGICKA: Self = Self(25);
    pub const STAMINA: Self = Self(26);
}

/// View of the actor values in the live engine.
pub struct ActorValueList<'a, S: ActorValueWorldState + ?Sized> {
    state: &'a mut S,
}

impl<'a, S: ActorValueWorldState + ?Sized> ActorValueList<'a, S> {
    pub fn new(state: &'a mut S) -> Self {
        Self { state }
    }

    pub fn actor_value(&self, actor: u32, value: ActorValue) -> Option<f32> {
        self.state.actor_value(actor, value)
    }

    pub fn set_actor_value(&mut self, actor: u32, value: ActorValue, amount: f32) -> bool {
        self.state.set_actor_value(actor, value, amount)
    }

    pub fn get_actor_value_info(&self, value: ActorValue) -> Option<String> {
        self.state.actor_value_name(value)
    }

    pub fn get_actor_value_name(&self, value: ActorValue) -> Option<String> {
        self.state.actor_value_name(value)
    }

    pub fn lookup_actor_value_by_name(&self, name: &str) -> Option<ActorValue> {
        self.state.actor_value_by_name(name)
    }
}

/// View of record-backed default objects in the live engine.
pub struct BgsDefaultObjectManager<'a, S: ActorValueWorldState + ?Sized> {
    state: &'a S,
}

impl<'a, S: ActorValueWorldState + ?Sized> BgsDefaultObjectManager<'a, S> {
    pub fn new(state: &'a S) -> Self {
        Self { state }
    }

    pub fn get(&self, key: [u8; 4]) -> Option<u32> {
        self.state.default_object(key)
    }
}

/// Hooks that require Skyrim's projectile runtime are explicitly unavailable.
pub struct ArrowProjectile;

impl ArrowProjectile {
    pub fn save_game() -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook::ArrowProjectile)
    }

    pub fn load_game() -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook::ArrowProjectile)
    }

    pub fn process_impacts() -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook::ArrowProjectile)
    }
}

/// Hooks that depend on Skyrim's binary load-form buffer are unavailable.
pub struct BgsLoadFormBuffer;

impl BgsLoadFormBuffer {
    pub fn read_version() -> Result<u8, UnsupportedHook> {
        Err(UnsupportedHook::LoadFormBuffer)
    }

    pub fn load_form() -> Result<u32, UnsupportedHook> {
        Err(UnsupportedHook::LoadFormBuffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct State {
        health: f32,
        gold: u32,
    }

    impl ActorValueWorldState for State {
        fn actor_value(&self, _actor: u32, value: ActorValue) -> Option<f32> {
            (value == ActorValue::HEALTH).then_some(self.health)
        }

        fn set_actor_value(&mut self, _actor: u32, value: ActorValue, amount: f32) -> bool {
            if value != ActorValue::HEALTH {
                return false;
            }
            self.health = amount;
            true
        }

        fn actor_value_name(&self, value: ActorValue) -> Option<String> {
            (value == ActorValue::HEALTH).then(|| "health".to_string())
        }

        fn actor_value_by_name(&self, name: &str) -> Option<ActorValue> {
            name.eq_ignore_ascii_case("health")
                .then_some(ActorValue::HEALTH)
        }

        fn default_object(&self, key: [u8; 4]) -> Option<u32> {
            (key == *b"GOLD").then_some(self.gold)
        }
    }

    #[test]
    fn shm_05_actor_value_and_default_object_views_use_live_state() {
        let mut state = State {
            health: 80.0,
            gold: 0x14,
        };
        {
            let mut values = ActorValueList::new(&mut state);
            assert_eq!(values.actor_value(7, ActorValue::HEALTH), Some(80.0));
            assert!(values.set_actor_value(7, ActorValue::HEALTH, 65.0));
            assert_eq!(
                values.get_actor_value_info(ActorValue::HEALTH).as_deref(),
                Some("health")
            );
            assert_eq!(
                values.get_actor_value_name(ActorValue::HEALTH).as_deref(),
                Some("health")
            );
            assert_eq!(
                values.lookup_actor_value_by_name("HEALTH"),
                Some(ActorValue::HEALTH)
            );
        }
        assert_eq!(
            BgsDefaultObjectManager::new(&state).get(*b"GOLD"),
            Some(0x14)
        );
        assert_eq!(state.health, 65.0);
    }

    #[test]
    fn shm_05_unavailable_projectile_and_load_buffer_hooks_return_typed_errors() {
        assert_eq!(
            ArrowProjectile::save_game(),
            Err(UnsupportedHook::ArrowProjectile)
        );
        assert_eq!(
            ArrowProjectile::load_game(),
            Err(UnsupportedHook::ArrowProjectile)
        );
        assert_eq!(
            ArrowProjectile::process_impacts(),
            Err(UnsupportedHook::ArrowProjectile)
        );
        assert_eq!(
            BgsLoadFormBuffer::read_version(),
            Err(UnsupportedHook::LoadFormBuffer)
        );
        assert_eq!(
            BgsLoadFormBuffer::load_form(),
            Err(UnsupportedHook::LoadFormBuffer)
        );
    }
}
