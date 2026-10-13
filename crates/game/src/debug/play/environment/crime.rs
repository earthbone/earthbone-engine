// SPDX-License-Identifier: GPL-3.0-or-later

//! UESP crime console arguments map to the shared faction ledger and jail markers.

use super::{Args, Console, Game, Outcome};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "setcrimegold" | "paycrimegold" | "getcrimegold" | "modcrimegold"
    )
}

impl Console {
    pub(super) fn crime_command(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
        args: &Args<'_>,
    ) -> Result<Outcome, String> {
        let amount = if matches!(name, "setcrimegold" | "modcrimegold") {
            args.int(0)?
        } else {
            0
        };
        let index = match name {
            "paycrimegold" => 2,
            "getcrimegold" => 0,
            _ => 1,
        };
        let faction = if args.0.get(index).is_some() {
            args.form(game, index)?
        } else {
            let selected = self.target_of(game, target)?;
            game.world_mut()
                .actor(selected)
                .and_then(|a| a.crime_faction)
                .ok_or("selected actor has no crime faction")?
        };
        if name == "paycrimegold" {
            game.world_mut().console_pay_crime_gold(
                faction,
                args.flag(0, true)?,
                args.flag(1, false)?,
            )?;
            return Ok(Outcome::Done);
        }
        let mut world = game.world_mut();
        let state = world.faction(faction).ok_or("requires a faction")?;
        match name {
            "setcrimegold" => {
                state.crime_gold_nonviolent = amount.max(0);
                state.crime_gold_violent = 0;
            }
            "modcrimegold" => {
                state.crime_gold_nonviolent =
                    state.crime_gold_nonviolent.saturating_add(amount).max(0);
            }
            _ => {
                return Ok(Outcome::Int(i64::from(
                    state
                        .crime_gold_nonviolent
                        .saturating_add(state.crime_gold_violent),
                )))
            }
        }
        Ok(Outcome::Done)
    }
}
