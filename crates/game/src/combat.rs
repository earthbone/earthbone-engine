// SPDX-License-Identifier: GPL-3.0-or-later

//! Physical melee contacts from winning WEAP/ARMO and GMST records.
//! xEdit/UESP describe fields. Published Skyrim GMST measurements describe
//! skill scaling and armor reduction, with source links in docs/combat.md.

pub mod records;

use soe_esm::GlobalFormId;

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
    pub health_damage: f32,
    pub killed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Record(#[from] soe_esm::Error),
    #[error(transparent)]
    Value(#[from] crate::actor_values::Error),
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
