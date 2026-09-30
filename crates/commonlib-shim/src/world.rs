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
    /// Activate-reference child data has no runtime representation.
    ActivateReferenceChildren,
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
// World-facing CommonLib contracts for save state and calculated leveled items.
// The API names the supported state and reports hooks without an engine path.

/// A save payload owned by the running game world.
pub trait BGSSaveLoadGame {
    fn save_state(&self) -> Vec<u8>;
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), String>;

    fn get_change(&self) -> Result<bool, NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BGSSaveLoadGame::GetChange"))
    }

    fn is_form_id_in_use(&self, _form_id: u32) -> Result<bool, NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BGSSaveLoadGame::IsFormIDInUse"))
    }
}

/// Save and load manager operations available to source-level plugins.
pub trait BGSSaveLoadManager: BGSSaveLoadGame {
    fn save(&self) -> Vec<u8> {
        self.save_state()
    }

    fn load(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.load_state(bytes)
    }

    fn save_file(&self) -> Result<(), NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BGSSaveLoadManager::Save"))
    }

    fn load_file(&mut self) -> Result<(), NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BGSSaveLoadManager::Load"))
    }

    fn load_most_recent_save_game(&mut self) -> Result<(), NamedUnsupportedHook> {
        Err(NamedUnsupportedHook(
            "BGSSaveLoadManager::LoadMostRecentSaveGame",
        ))
    }

    fn populate_save_list(&mut self) -> Result<(), NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BGSSaveLoadManager::PopulateSaveList"))
    }
}

/// A calculated leveled-list item, corresponding to CommonLib `CALCED_OBJECT`.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CALCED_OBJECT {
    pub form_id: u32,
    pub count: u16,
    pub container_item: ContainerItemExtra,
}

/// The resolved COED metadata carried by a calculated list result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContainerItemExtra {
    pub owner: Option<u32>,
    pub owner_rank: Option<u32>,
    pub owner_global: Option<u32>,
    pub health_bits: Option<u32>,
}

/// Character-controller flags declared by CommonLib's Havok controller API.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CHARACTER_FLAGS(pub u32);

#[allow(non_upper_case_globals)]
impl CHARACTER_FLAGS {
    pub const kNone: Self = Self(0);
    pub const kQuadruped: Self = Self(1 << 0);
    pub const kNoGravityOnGround: Self = Self(1 << 1);
    pub const kTryStep: Self = Self(1 << 2);
    pub const kNoFriction: Self = Self(1 << 3);
    pub const kAllowJumpNoContact: Self = Self(1 << 4);
    pub const kStuckQuad: Self = Self(1 << 5);
    pub const kAnimAngleMod: Self = Self(1 << 6);
    pub const kHitDamage: Self = Self(1 << 7);
    pub const kHitFlags: Self = Self(1 << 7);
    pub const kSupport: Self = Self(1 << 8);
    pub const kHasPotentialSupportManifold: Self = Self(1 << 9);
    pub const kCanJump: Self = Self(1 << 10);
    pub const kChaseBip: Self = Self(1 << 11);
    pub const kFollowRagdoll: Self = Self(1 << 12);
    pub const kJumping: Self = Self(1 << 13);
    pub const kNotPushable: Self = Self(1 << 14);
    pub const kFloatLand: Self = Self(1 << 15);
    pub const kCheckSupport: Self = Self(1 << 16);
    pub const kNoSim: Self = Self(1 << 17);
    pub const kFarAway: Self = Self(1 << 18);
    pub const kOnStilts: Self = Self(1 << 19);
    pub const kQuickSimulate: Self = Self(1 << 20);
    pub const kRecordHits: Self = Self(1 << 21);
    pub const kComputeTiltPreIntegrate: Self = Self(1 << 22);
    pub const kShouldersUnderWater: Self = Self(1 << 23);
    pub const kOnStairs: Self = Self(1 << 24);
    pub const kCanPitch: Self = Self(1 << 25);
    pub const kCanRoll: Self = Self(1 << 26);
    pub const kNoCharacterCollisions: Self = Self(1 << 27);
    pub const kNotPushablePermanent: Self = Self(1 << 28);
    pub const kPossiblePathObstacle: Self = Self(1 << 29);
    pub const kShapeRequiresZRot: Self = Self(1 << 30);
    pub const kSwimAtWaterSurface: Self = Self(1 << 31);
}

/// A hook that has no matching runtime behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedUnsupportedHook(pub &'static str);

impl std::fmt::Display for NamedUnsupportedHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported CommonLib hook: {}", self.0)
    }
}

impl std::error::Error for NamedUnsupportedHook {}

/// The game-owned state that this batch can expose through the MIT boundary.
pub trait WorldBindings: BGSSaveLoadManager {
    fn calculate_current_form_list(
        &mut self,
        list: u32,
        level: u16,
        count: i16,
    ) -> Result<Vec<CALCED_OBJECT>, String>;

    fn beam_projectile_impact(&mut self) -> Result<(), NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("BeamProjectile::AddImpact"))
    }

    fn character_flags(&self) -> Result<u32, NamedUnsupportedHook> {
        Err(NamedUnsupportedHook("bhkCharacterController::flags"))
    }
}

impl<T: BGSSaveLoadGame + ?Sized> BGSSaveLoadManager for T {}

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

    #[test]
    fn shm_06_unsupported_hooks_name_the_missing_runtime_paths() {
        let projectile = NamedUnsupportedHook("BeamProjectile::AddImpact");
        assert_eq!(
            projectile.to_string(),
            "unsupported CommonLib hook: BeamProjectile::AddImpact"
        );
        let flags = [
            CHARACTER_FLAGS::kNone,
            CHARACTER_FLAGS::kQuadruped,
            CHARACTER_FLAGS::kNoGravityOnGround,
            CHARACTER_FLAGS::kTryStep,
            CHARACTER_FLAGS::kNoFriction,
            CHARACTER_FLAGS::kAllowJumpNoContact,
            CHARACTER_FLAGS::kStuckQuad,
            CHARACTER_FLAGS::kAnimAngleMod,
            CHARACTER_FLAGS::kHitDamage,
            CHARACTER_FLAGS::kSupport,
            CHARACTER_FLAGS::kHasPotentialSupportManifold,
            CHARACTER_FLAGS::kCanJump,
            CHARACTER_FLAGS::kChaseBip,
            CHARACTER_FLAGS::kFollowRagdoll,
            CHARACTER_FLAGS::kJumping,
            CHARACTER_FLAGS::kNotPushable,
            CHARACTER_FLAGS::kFloatLand,
            CHARACTER_FLAGS::kCheckSupport,
            CHARACTER_FLAGS::kNoSim,
            CHARACTER_FLAGS::kFarAway,
            CHARACTER_FLAGS::kOnStilts,
            CHARACTER_FLAGS::kQuickSimulate,
            CHARACTER_FLAGS::kRecordHits,
            CHARACTER_FLAGS::kComputeTiltPreIntegrate,
            CHARACTER_FLAGS::kShouldersUnderWater,
            CHARACTER_FLAGS::kOnStairs,
            CHARACTER_FLAGS::kCanPitch,
            CHARACTER_FLAGS::kCanRoll,
            CHARACTER_FLAGS::kNoCharacterCollisions,
            CHARACTER_FLAGS::kNotPushablePermanent,
            CHARACTER_FLAGS::kPossiblePathObstacle,
            CHARACTER_FLAGS::kShapeRequiresZRot,
            CHARACTER_FLAGS::kSwimAtWaterSurface,
        ];
        for (bit, flag) in flags.into_iter().enumerate().skip(1) {
            assert_eq!(flag.0, 1 << (bit - 1), "flag bit {bit}");
        }
        assert_eq!(CHARACTER_FLAGS::kNone.0, 0);
        assert_eq!(CHARACTER_FLAGS::kHitFlags.0, CHARACTER_FLAGS::kHitDamage.0);
    }
}

/// Calendar values exposed by the running game.
pub trait Calendar {
    fn year(&self) -> u32;
    fn month(&self) -> u32;
    fn day(&self) -> u32;
    fn hour(&self) -> f64;
    fn days_passed(&self) -> f64;
    fn timescale(&self) -> f64;
}

/// Actor state exposed through the engine's actor component.
pub trait Character {
    fn form_id(&self) -> FormId;
    fn actor_value(&self, name: &str) -> Option<f32>;
    fn is_dead(&self) -> bool;
    fn race(&self) -> Option<FormId>;
}

/// Sparse `DOBJ` lookup by the four byte type key.
pub trait DefaultObjects {
    fn get(&self, key: [u8; 4]) -> Result<Option<FormId>, String>;
}

/// The runtime reference-alias data represented by ExtraAliasInstanceArray.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasInstance {
    pub quest: FormId,
    pub alias_id: u32,
    pub reference: Option<FormId>,
    pub packages: Vec<FormId>,
}

pub trait AliasInstances {
    fn instances(&self, reference: FormId) -> Vec<AliasInstance>;
}

/// Child activation data is not currently stored by the engine runtime.
/// Callers receive an explicit unsupported result rather than a false empty list.
pub trait ActivateRefChildren {
    fn children(&self, reference: FormId) -> Result<Vec<FormId>, UnsupportedHook>;
    fn timer(&self, reference: FormId) -> Result<f32, UnsupportedHook>;
    fn activate_children(&self, reference: FormId) -> Result<(), UnsupportedHook>;
}
