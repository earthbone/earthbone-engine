// SPDX-License-Identifier: MIT

//! World-facing CommonLib contracts for save state and calculated leveled items.
//! The API names the supported state and reports hooks without an engine path.

/// A save payload owned by the running game world.
pub trait BGSSaveLoadGame {
    fn save_state(&self) -> Vec<u8>;
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), String>;

    fn get_change(&self) -> Result<bool, UnsupportedHook> {
        Err(UnsupportedHook("BGSSaveLoadGame::GetChange"))
    }

    fn is_form_id_in_use(&self, _form_id: u32) -> Result<bool, UnsupportedHook> {
        Err(UnsupportedHook("BGSSaveLoadGame::IsFormIDInUse"))
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

    fn save_file(&self) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook("BGSSaveLoadManager::Save"))
    }

    fn load_file(&mut self) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook("BGSSaveLoadManager::Load"))
    }

    fn load_most_recent_save_game(&mut self) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook(
            "BGSSaveLoadManager::LoadMostRecentSaveGame",
        ))
    }

    fn populate_save_list(&mut self) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook("BGSSaveLoadManager::PopulateSaveList"))
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
pub struct UnsupportedHook(pub &'static str);

impl std::fmt::Display for UnsupportedHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported CommonLib hook: {}", self.0)
    }
}

impl std::error::Error for UnsupportedHook {}

/// The game-owned state that this batch can expose through the MIT boundary.
pub trait WorldBindings: BGSSaveLoadManager {
    fn calculate_current_form_list(
        &mut self,
        list: u32,
        level: u16,
        count: i16,
    ) -> Result<Vec<CALCED_OBJECT>, String>;

    fn beam_projectile_impact(&mut self) -> Result<(), UnsupportedHook> {
        Err(UnsupportedHook("BeamProjectile::AddImpact"))
    }

    fn character_flags(&self) -> Result<u32, UnsupportedHook> {
        Err(UnsupportedHook("bhkCharacterController::flags"))
    }
}

impl<T: BGSSaveLoadGame + ?Sized> BGSSaveLoadManager for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_06_unsupported_hooks_name_the_missing_runtime_paths() {
        let projectile = UnsupportedHook("BeamProjectile::AddImpact");
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
