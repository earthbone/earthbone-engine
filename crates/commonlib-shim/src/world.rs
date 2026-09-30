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
