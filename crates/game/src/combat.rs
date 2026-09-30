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
    /// The contact is a shield bash rather than an ordinary weapon swing.
    /// `weapon` then names the attacker's equipped shield ARMO instead of a
    /// WEAP record. See docs/combat.md CMB-20.
    pub is_bash: bool,
    /// The attacker's animation graph confirmed a power attack for this
    /// swing, the same host-confirmed pattern `World::set_blocking` uses
    /// for the target's block stance. Not derived here from an actor value
    /// such as `IsPowerAttacking`, see docs/combat.md CMB-21.
    pub is_power_attack: bool,
}

/// Record-backed request to start one arrow or bolt flight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightRequest {
    /// Actor that released the projectile.
    pub attacker: GlobalFormId,
    /// Bow or crossbow used for the release.
    pub weapon: GlobalFormId,
    /// One consumed AMMO form.
    pub ammunition: GlobalFormId,
    /// Winning PROJ record linked by the ammunition.
    pub projectile: GlobalFormId,
    /// World-space spawn point supplied by the host.
    pub origin: [f32; 3],
    /// Normalized world-space launch direction.
    pub direction: [f32; 3],
    /// PROJ gravity value.
    pub gravity: f32,
    /// PROJ launch speed.
    pub speed: f32,
    /// PROJ maximum range.
    pub range: f32,
    /// AMMO base damage.
    pub damage: f32,
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
    /// Fraction of damage the target's matching resistance actor value
    /// removed, from the weapon's DNAM Resist field. 0 when the field is -1
    /// or the weapon's "Ignores Normal Weapon Resistance" flag suppressed
    /// it. Stacks multiplicatively with `armor_reduction` and
    /// `block_reduction`. See docs/combat.md CMB-26.
    pub resistance_reduction: f32,
    pub health_damage: f32,
    pub killed: bool,
    /// Post armor and block damage crossed `fStaggerThreshold` scaled by the
    /// weapon's DNAM Stagger fraction, setting the target's
    /// `ActorState::stagger_end_time`. See docs/combat.md CMB-14.
    pub staggered: bool,
    /// This hit was a shield bash, reported for OnHit's abBashAttack
    /// argument. See docs/combat.md CMB-20.
    pub bashed: bool,
    /// The equipped weapon carried a poisoned stack (CommonLib ExtraPoison,
    /// kind 62) and this hit applied its effects and cleared the coating.
    /// Always false for a bash or an unarmed contact. See docs/combat.md
    /// CMB-25.
    pub poisoned: bool,
    /// The attacker was sneaking and the target had not entered Combat
    /// awareness of that attacker. Reports OnHit's `abSneakAttack` value.
    pub sneak_attacked: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Record(#[from] soe_esm::Error),
    #[error(transparent)]
    Value(#[from] crate::actor_values::Error),
    #[error(transparent)]
    Inventory(#[from] crate::inventory::Error),
    #[error(transparent)]
    Magic(#[from] crate::world::magic::MagicError),
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
    #[error("perk entry point: {0}")]
    Perk(#[from] crate::perks::DispatchError),
}
