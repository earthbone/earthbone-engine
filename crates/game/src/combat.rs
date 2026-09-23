// SPDX-License-Identifier: GPL-3.0-or-later

//! Physical melee contacts from winning WEAP/ARMO and GMST records.
//! xEdit/UESP describe fields. Published Skyrim GMST measurements describe
//! skill scaling and armor reduction, with source links in docs/combat.md.

pub mod records;

use soe_esm::GlobalFormId;

/// The six Skyrim damage difficulty tiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Difficulty {
    Novice,
    Apprentice,
    #[default]
    Adept,
    Expert,
    Master,
    Legendary,
}

impl Difficulty {
    pub(crate) const fn gmst_names(self) -> (&'static str, &'static str) {
        match self {
            Self::Novice => ("fDiffMultHPByPCVE", "fDiffMultHPToPCVE"),
            Self::Apprentice => ("fDiffMultHPByPCE", "fDiffMultHPToPCE"),
            Self::Adept => ("fDiffMultHPByPCN", "fDiffMultHPToPCN"),
            Self::Expert => ("fDiffMultHPByPCH", "fDiffMultHPToPCH"),
            Self::Master => ("fDiffMultHPByPCVH", "fDiffMultHPToPCVH"),
            Self::Legendary => ("fDiffMultHPByPCL", "fDiffMultHPToPCL"),
        }
    }

    pub(crate) const fn from_save(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Novice),
            1 => Some(Self::Apprentice),
            2 => Some(Self::Adept),
            3 => Some(Self::Expert),
            4 => Some(Self::Master),
            5 => Some(Self::Legendary),
            _ => None,
        }
    }
}

/// Weapon hand, matching CK GetEquippedWeapon's left-hand argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Left,
    Right,
}

/// Host-confirmed contact. Spatial detection and animation choose the weapon.
#[derive(Debug, Clone, Copy)]
pub struct MeleeHit {
    pub attacker: GlobalFormId,
    pub target: GlobalFormId,
    pub weapon: GlobalFormId,
}

/// Components of one applied physical hit, before any later script effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitResult {
    pub weapon_damage: f32,
    pub armor_rating: f32,
    pub armor_reduction: f32,
    /// The target held a block stance with a shield equipped. Weapon-only
    /// blocking (no shield) is not evaluated, see docs/combat.md.
    pub blocked: bool,
    /// Fraction of damage the shield block removed, 0 when `blocked` is false.
    /// Stacks multiplicatively with `armor_reduction`.
    pub block_reduction: f32,
    pub health_damage: f32,
    pub killed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Record(#[from] soe_esm::Error),
    #[error(transparent)]
    Value(#[from] crate::actor_values::Error),
    #[error(transparent)]
    Inventory(#[from] crate::inventory::Error),
    #[error("invalid combat actor {0}")]
    Actor(GlobalFormId),
    #[error("weapon is not equipped or present in the attacker's inventory")]
    Equipment,
    #[error("missing, invalid or negative combat game setting {0}")]
    Setting(&'static str),
    #[error("non-finite or overflowing combat calculation")]
    Number,
    #[error("unsupported combat behavior: {0}")]
    Unsupported(&'static str),
}
