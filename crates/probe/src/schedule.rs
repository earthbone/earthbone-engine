// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-hour actor schedule captures and row-wise differential reports.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Error;

/// One actor's observable schedule state at a game-hour boundary.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScheduleRow {
    pub hour: u8,
    pub actor: u32,
    pub name: String,
    pub package: Option<u32>,
    pub cell: Option<u32>,
    pub furniture: Option<u32>,
}

/// A sorted table of actor states, one row per unique actor and game hour.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScheduleTable {
    pub rows: Vec<ScheduleRow>,
}

/// Fields that differ for one actor-hour pair.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScheduleDifference {
    pub hour: u8,
    pub actor: u32,
    pub name: String,
    pub field: String,
    pub engine: Option<u32>,
    pub observed: Option<u32>,
}

impl ScheduleTable {
    /// Validate a complete 24-hour table and sort rows into stable order.
    ///
    /// # Errors
    /// Rejects invalid hours, empty names, duplicate actor-hour keys, or an
    /// actor whose name changes during the day.
    pub fn new(mut rows: Vec<ScheduleRow>) -> Result<Self, Error> {
        validate_rows(&mut rows, true)?;
        Ok(Self { rows })
    }

    fn observed(mut rows: Vec<ScheduleRow>) -> Result<Self, Error> {
        validate_rows(&mut rows, false)?;
        Ok(Self { rows })
    }

    /// Compare every actor-hour and list every changed, missing, or extra field.
    #[must_use]
    pub fn differences(&self, observed: &Self) -> Vec<ScheduleDifference> {
        let expected: BTreeMap<_, _> = self
            .rows
            .iter()
            .map(|row| ((row.hour, row.actor), row))
            .collect();
        let actual: BTreeMap<_, _> = observed
            .rows
            .iter()
            .map(|row| ((row.hour, row.actor), row))
            .collect();
        let keys: BTreeSet<_> = expected.keys().chain(actual.keys()).copied().collect();
        let mut differences = Vec::new();
        for (hour, actor) in keys {
            let engine = expected.get(&(hour, actor)).copied();
            let observed = actual.get(&(hour, actor)).copied();
            let name = engine
                .or(observed)
                .map_or_else(String::new, |row| row.name.clone());
            for (field, engine_value, observed_value) in [
                ("row", engine.map(|_| 1), observed.map(|_| 1)),
                (
                    "package",
                    engine.and_then(|row| row.package),
                    observed.and_then(|row| row.package),
                ),
                (
                    "cell",
                    engine.and_then(|row| row.cell),
                    observed.and_then(|row| row.cell),
                ),
                (
                    "furniture",
                    engine.and_then(|row| row.furniture),
                    observed.and_then(|row| row.furniture),
                ),
            ] {
                if engine_value != observed_value {
                    differences.push(ScheduleDifference {
                        hour,
                        actor,
                        name: name.clone(),
                        field: field.into(),
                        engine: engine_value,
                        observed: observed_value,
                    });
                }
            }
        }
        differences
    }
}

fn validate_rows(rows: &mut [ScheduleRow], require_complete_day: bool) -> Result<(), Error> {
    rows.sort_by_key(|row| (row.hour, row.actor));
    let mut seen = BTreeSet::new();
    let mut names = BTreeMap::new();
    for row in rows.iter() {
        if row.hour >= 24 || row.name.trim().is_empty() {
            return Err(Error::Invalid("schedule row hour or actor name".into()));
        }
        if !seen.insert((row.hour, row.actor)) {
            return Err(Error::Invalid("duplicate schedule actor-hour row".into()));
        }
        if names
            .insert(row.actor, &row.name)
            .is_some_and(|name| name != &row.name)
        {
            return Err(Error::Invalid("schedule actor name changed".into()));
        }
    }
    let actors: BTreeSet<_> = rows.iter().map(|row| row.actor).collect();
    if require_complete_day
        && (rows.len() != actors.len() * 24
            || actors
                .iter()
                .any(|actor| (0..24).any(|hour| !seen.contains(&(hour, *actor)))))
    {
        return Err(Error::Invalid(
            "schedule table must contain every actor-hour".into(),
        ));
    }
    Ok(())
}

/// Parse `SOESCHEDULE ` JSON rows from a Papyrus log.
///
/// # Errors
/// Rejects malformed rows and duplicate actor-hour samples.
pub fn parse_schedule_log(log: &str) -> Result<ScheduleTable, Error> {
    let mut rows = Vec::new();
    for (index, line) in log.lines().enumerate() {
        let Some((_, json)) = line.split_once("SOESCHEDULE ") else {
            continue;
        };
        let row: ScheduleRow = serde_json::from_str(json).map_err(|error| Error::Observation {
            line: index + 1,
            reason: error.to_string(),
        })?;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(Error::Observation {
            line: 0,
            reason: "no SOESCHEDULE rows".into(),
        });
    }
    ScheduleTable::observed(rows)
}
