// SPDX-License-Identifier: MIT

//! Runtime world contracts shared with the engine without importing core types.

/// Stable form identity at the shim boundary. The two components preserve the
/// load-order identity instead of treating an object number as globally unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormKey {
    pub file: u16,
    pub object: u32,
}

/// Live world reads used by this batch of CommonLib-shaped types.
pub trait WorldState {
    fn player(&mut self) -> Option<FormKey>;
    fn loaded_actors(&self) -> Vec<FormKey>;
    fn reference_position(&mut self, form: FormKey) -> Option<[f32; 3]>;
    fn actor_value(&mut self, form: FormKey, name: &str) -> Option<f32>;
    fn actor_is_dead(&mut self, form: FormKey) -> Option<bool>;
}

/// The unsupported part of a CommonLib hook request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedHook {
    /// Projectile creation, simulation and manager access are not implemented.
    ProjectileRuntime,
    /// Binary call-site and vtable patching cannot target this runtime.
    BinaryPatch,
}

/// A hook rejected by the source-level shim boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsupported {
    pub hook: UnsupportedHook,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.hook {
            UnsupportedHook::ProjectileRuntime => f.write_str("projectile runtime is unsupported"),
            UnsupportedHook::BinaryPatch => f.write_str("binary patch hooks are unsupported"),
        }
    }
}

impl std::error::Error for Unsupported {}

/// CommonLib's typed object-reference handle representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ObjectRefHandle(pub u32);

/// CommonLib's typed projectile handle representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProjectileHandle(pub u32);

/// Read-only subset of the player singleton supported by the engine.
pub struct PlayerCharacter<'a, W: WorldState> {
    world: &'a mut W,
}

impl<'a, W: WorldState> PlayerCharacter<'a, W> {
    pub fn new(world: &'a mut W) -> Option<Self> {
        world.player()?;
        Some(Self { world })
    }

    pub fn form_key(&mut self) -> Option<FormKey> {
        self.world.player()
    }

    pub fn actor_value(&mut self, name: &str) -> Option<f32> {
        let player = self.world.player()?;
        self.world.actor_value(player, name)
    }

    pub fn position(&mut self) -> Option<[f32; 3]> {
        let player = self.world.player()?;
        self.world.reference_position(player)
    }
}

/// The actor collection currently driven by the engine's world model.
pub struct ProcessLists<'a, W: WorldState> {
    world: &'a W,
}

impl<'a, W: WorldState> ProcessLists<'a, W> {
    pub fn new(world: &'a W) -> Self {
        Self { world }
    }

    pub fn actor_forms(&self) -> Vec<FormKey> {
        self.world.loaded_actors()
    }
}

/// A projectile runtime object is named for source compatibility, while its
/// runtime operations fail explicitly until projectile simulation is added.
pub struct Projectile {
    pub handle: ProjectileHandle,
}

impl Projectile {
    pub fn runtime(&self) -> Result<(), Unsupported> {
        Err(Unsupported {
            hook: UnsupportedHook::ProjectileRuntime,
        })
    }
}
