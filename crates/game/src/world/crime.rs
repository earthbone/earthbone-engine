// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording a witnessed crime. See `crate::crime` for the crime types, the
//! `FACT.CRVA` pricing table and this module's `Error`.

use soe_esm::GlobalFormId;

use crate::crime::{CrimeType, Error};

use super::World;

impl World {
    /// Records a crime `perpetrator` commits, seen by `witness`. Prices it
    /// from `perpetrator`'s own crime faction's `CRVA` table
    /// (`Actor.GetCrimeFaction`) and adds the result to that faction's
    /// bounty alone, the violent or nonviolent split `Faction.ModCrimeGold`
    /// already keeps. A faction sharing the perpetrator's `CRGR` crime
    /// group is never touched, `CRGR` is not parsed (see docs/game.md).
    /// Marks `witness` as knowing `perpetrator` did it, in
    /// `ActorState::witnessed_crimes`, which persists across save and load.
    ///
    /// Every check runs before anything is mutated: an invalid witness or
    /// perpetrator, a perpetrator with no crime faction, or a crime faction
    /// that does not track crime all leave the world unchanged.
    ///
    /// The bounty and infamy ledgers use saturating arithmetic and clamp at
    /// zero, so a legitimately priced crime on top of an existing bounty
    /// never overflows the running total, matching `CrimeType::bounty`'s own
    /// saturating float-to-int cast in `crate::crime`.
    ///
    /// # Errors
    /// Returns [`Error::Actor`] when `witness` or `perpetrator` is not a
    /// valid actor reference, [`Error::NoCrimeFaction`] when `perpetrator`
    /// has none set, and [`Error::NotTracked`] when its crime faction does
    /// not exist or does not track crime.
    pub fn record_witnessed_crime(
        &mut self,
        perpetrator: GlobalFormId,
        witness: GlobalFormId,
        crime: CrimeType,
    ) -> Result<i32, Error> {
        self.actor(witness).ok_or(Error::Actor(witness))?;
        let faction_id = self
            .actor(perpetrator)
            .ok_or(Error::Actor(perpetrator))?
            .crime_faction
            .ok_or(Error::NoCrimeFaction(perpetrator))?;
        self.record_witnessed_crime_in_faction(perpetrator, witness, crime, faction_id)
    }

    /// Price a witnessed crime for the faction the witness reports to.
    pub(super) fn record_witnessed_crime_in_faction(
        &mut self,
        perpetrator: GlobalFormId,
        witness: GlobalFormId,
        crime: CrimeType,
        faction_id: GlobalFormId,
    ) -> Result<i32, Error> {
        self.actor(witness).ok_or(Error::Actor(witness))?;
        self.actor(perpetrator).ok_or(Error::Actor(perpetrator))?;
        let values = self.crime_values(faction_id);
        let fs = self
            .faction(faction_id)
            .ok_or(Error::NotTracked(faction_id))?;
        if !fs.tracks_crime {
            return Err(Error::NotTracked(faction_id));
        }
        let amount = crime.bounty(&values);
        if crime.is_violent() {
            fs.crime_gold_violent = fs.crime_gold_violent.saturating_add(amount).max(0);
            fs.infamy_violent = fs.infamy_violent.saturating_add(amount.max(0));
        } else {
            fs.crime_gold_nonviolent = fs.crime_gold_nonviolent.saturating_add(amount).max(0);
            fs.infamy_nonviolent = fs.infamy_nonviolent.saturating_add(amount.max(0));
        }
        let total = fs
            .crime_gold_violent
            .saturating_add(fs.crime_gold_nonviolent);
        self.actor(witness)
            .ok_or(Error::Actor(witness))?
            .witnessed_crimes
            .insert(perpetrator);
        Ok(total)
    }
}
