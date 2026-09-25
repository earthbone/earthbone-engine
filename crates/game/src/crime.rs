// SPDX-License-Identifier: GPL-3.0-or-later

//! Crime types and the `FACT.CRVA` bounty table they are priced against.
//!
//! Layout: `wbRecord(FACT` in `TES5Edit/Core/wbDefinitionsTES5.pas` and
//! `uesp-modfileformat/FACT.wikitext`. `CRVA` is a struct whose size grew
//! across record versions, 12, 16 or 20 bytes, earlier fields kept as later
//! ones are appended.
//!
//! `Actor.GetCrimeFaction`/`SetCrimeFaction` (CK wiki, same page names) say
//! an actor reports its own crimes to one faction. `World::record_witnessed_crime`
//! (`crates/game/src/world/crime.rs`) prices a crime against that faction's
//! `CRVA` table and changes only that faction's bounty, never a faction that
//! shares the perpetrator's `CRGR` crime group. `CRGR` is not parsed here,
//! see the open question in `docs/game.md`.

use soe_esm::GlobalFormId;

/// Decoded `FACT.CRVA`. Arrest and attack-on-sight are the faction's
/// reputation policy, not bounty amounts.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CrimeValues {
    pub arrest: bool,
    pub attack_on_sight: bool,
    pub murder: u16,
    pub assault: u16,
    pub trespass: u16,
    pub pickpocket: u16,
    pub steal_multiplier: f32,
    pub escape: u16,
    pub werewolf: u16,
}

impl CrimeValues {
    /// Decodes a `CRVA` struct of 12, 16 or 20 bytes. UESP: fields stay in
    /// declaration order as the struct grows across record versions, so a
    /// short struct still lines up with the start of a long one: 12 bytes
    /// covers arrest through pickpocket plus an unused field, 16 adds the
    /// steal multiplier, 20 adds escape and werewolf.
    ///
    /// # Errors
    /// Returns [`Error::CrvaSize`] for any other length.
    pub fn from_bytes(d: &[u8]) -> Result<Self, Error> {
        if !matches!(d.len(), 12 | 16 | 20) {
            return Err(Error::CrvaSize(d.len()));
        }
        let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]);
        let mut v = Self {
            arrest: d[0] != 0,
            attack_on_sight: d[1] != 0,
            murder: u16_at(2),
            assault: u16_at(4),
            trespass: u16_at(6),
            pickpocket: u16_at(8),
            ..Self::default()
        };
        if d.len() >= 16 {
            v.steal_multiplier = f32::from_le_bytes([d[12], d[13], d[14], d[15]]);
        }
        if d.len() == 20 {
            v.escape = u16_at(16);
            v.werewolf = u16_at(18);
        }
        Ok(v)
    }
}

/// A crime, naming the `CRVA` field (or, for stealing, the stolen item's
/// value) it is priced against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrimeType {
    Murder,
    Assault,
    Trespass,
    /// Pickpocketing carries its own fixed `CRVA` amount, unlike general
    /// theft.
    Pickpocket,
    /// General theft. `item_value` is priced by the faction's steal
    /// multiplier, unlike `Pickpocket`'s fixed amount.
    Stealing {
        item_value: u32,
    },
    Escape,
    Werewolf,
}

impl CrimeType {
    /// Violent crimes grow `FactionState::crime_gold_violent`, the rest
    /// grow `crime_gold_nonviolent`, the same split
    /// `Faction.ModCrimeGold`'s `abViolent` argument already keeps.
    #[must_use]
    pub const fn is_violent(self) -> bool {
        matches!(self, Self::Murder | Self::Assault)
    }

    /// The bounty this crime adds under `values`, before the caller clamps
    /// the faction's running total at zero.
    #[must_use]
    pub fn bounty(self, values: &CrimeValues) -> i32 {
        match self {
            Self::Murder => i32::from(values.murder),
            Self::Assault => i32::from(values.assault),
            Self::Trespass => i32::from(values.trespass),
            Self::Pickpocket => i32::from(values.pickpocket),
            Self::Stealing { item_value } => {
                (f64::from(item_value) * f64::from(values.steal_multiplier)).round() as i32
            }
            Self::Escape => i32::from(values.escape),
            Self::Werewolf => i32::from(values.werewolf),
        }
    }
}

/// Failure to decode a `CRVA` table or to record a witnessed crime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("FACT CRVA is {0} bytes, expected 12, 16 or 20")]
    CrvaSize(usize),
    #[error("{0} is not a valid actor reference")]
    Actor(GlobalFormId),
    #[error("{0} has no crime faction set")]
    NoCrimeFaction(GlobalFormId),
    #[error("faction {0} does not track crime")]
    NotTracked(GlobalFormId),
}
