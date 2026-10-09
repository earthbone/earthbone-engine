// SPDX-License-Identifier: GPL-3.0-or-later

//! Engine-side 24 game hour capture of every unique named NPC in Whiterun.
//!
//! The rows use the same shape as [`crate::ScheduleTable`], so the table the
//! engine records here can be compared hour by hour with a table recorded in
//! the original runtime.

use soe_esm::{GlobalFormId, Tag};
use soe_game::Game;

use crate::{Error, ScheduleRow, ScheduleTable};

/// Real seconds passed to each engine tick. At the default timescale of 20
/// this is 40 game seconds, so one game hour takes 90 ticks.
const TICK_SECONDS: f64 = 2.0;

/// Hard stop so a stalled clock fails instead of looping.
const MAX_TICKS: u32 = 4_000;

/// ACBS flag bit that marks an actor as unique.
const UNIQUE_FLAG: u32 = 0x20;

/// One unique named NPC placed in a Whiterun cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhiterunActor {
    /// The placed reference.
    pub reference: GlobalFormId,
    /// The NPC_ base record.
    pub base: GlobalFormId,
    /// Editor id of the base record, used as the row label.
    pub label: String,
}

/// List the unique named NPCs placed in every cell whose editor id starts
/// with `Whiterun`, one entry per base record, in reference order.
///
/// Attack and ruin variants of the city cells are skipped because they hold
/// the invasion population, not the daily schedule.
pub fn whiterun_actors(game: &mut Game) -> Vec<WhiterunActor> {
    let _ = game.world_mut().cell_by_editor_id("WhiterunExterior01");
    let cells: Vec<GlobalFormId> = {
        let world = game.world();
        let Some(index) = world.order().cell_index() else {
            return Vec::new();
        };
        index.cells().map(|(cell, _)| cell).collect()
    };
    let mut selected = Vec::new();
    for cell in cells {
        let Some(label) = game.world().editor_id(cell) else {
            continue;
        };
        if label.starts_with("Whiterun") && !label.contains("Attack") && !label.contains("Ruin") {
            selected.push(cell);
        }
    }
    selected.sort();
    let mut actors: Vec<WhiterunActor> = Vec::new();
    for cell in selected {
        let references = game.world_mut().cell_references(cell);
        for reference in references {
            if game.world().tag_of(reference) != Some(Tag(*b"ACHR")) {
                continue;
            }
            let Some(base) = game.world_mut().reference(reference).and_then(|r| r.base) else {
                continue;
            };
            if actors.iter().any(|actor| actor.base == base) {
                continue;
            }
            let world = game.world();
            let Some(record) = world.record(base) else {
                continue;
            };
            let flags = record
                .find(Tag(*b"ACBS"))
                .filter(|data| data.len() >= 4)
                .map(|data| u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
            let named = record.find(Tag(*b"FULL")).is_some();
            let Some(label) = world.editor_id(base) else {
                continue;
            };
            if named && flags.is_some_and(|flags| flags & UNIQUE_FLAG != 0) {
                actors.push(WhiterunActor {
                    reference,
                    base,
                    label,
                });
            }
        }
    }
    actors
}

/// Run the engine for 24 game hours and record each actor's running package,
/// cell and held furniture marker at the start of every game hour.
///
/// The first row of each actor is taken before any tick, then one set of rows
/// per crossed hour boundary until 24 hours are recorded. `hour` is the hour
/// of the game day, so a run that starts at 08:00 covers 08:00 to 07:00.
///
/// # Errors
/// Returns an error when the actor list is empty, when the clock does not
/// advance within the tick budget, or when the rows fail table validation.
pub fn capture_engine_day(
    game: &mut Game,
    actors: &[WhiterunActor],
) -> Result<ScheduleTable, Error> {
    if actors.is_empty() {
        return Err(Error::Invalid("no actors to record".into()));
    }
    for actor in actors {
        game.world_mut().drive_actor(actor.reference);
    }
    let mut rows = Vec::with_capacity(actors.len() * 24);
    let start = hour_index(game);
    record_hour(game, actors, start, &mut rows);
    let mut next = start + 1;
    for _ in 0..MAX_TICKS {
        if next >= start + 24 {
            break;
        }
        game.tick(TICK_SECONDS);
        let now = hour_index(game);
        while next <= now && next < start + 24 {
            record_hour(game, actors, next, &mut rows);
            next += 1;
        }
    }
    if next < start + 24 {
        return Err(Error::Invalid(format!(
            "game clock reached hour {} of {} within {MAX_TICKS} ticks",
            next - start,
            24
        )));
    }
    ScheduleTable::new(rows)
}

fn hour_index(game: &Game) -> u64 {
    (game.world().clock.game_days * 24.0).floor() as u64
}

fn record_hour(game: &Game, actors: &[WhiterunActor], index: u64, rows: &mut Vec<ScheduleRow>) {
    let world = game.world();
    let order = world.order();
    let runtime = |id: GlobalFormId| order.runtime_id(id);
    for actor in actors {
        let reference = world.reference_ref(actor.reference);
        rows.push(ScheduleRow {
            hour: (index % 24) as u8,
            actor: runtime(actor.reference),
            name: actor.label.clone(),
            package: world.current_package(actor.reference).map(runtime),
            cell: reference.and_then(|r| r.cell).map(runtime),
            furniture: world
                .furniture_seat(actor.reference)
                .map(|(marker, _)| runtime(marker)),
        });
    }
}
