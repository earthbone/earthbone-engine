// SPDX-License-Identifier: GPL-3.0-or-later

//! PLY-02 line grammar. No external file format is interpreted here.

use super::{Destination, Error, Marker, Route, Step};

const MAX_TICKS: u32 = 36_000;

impl Route {
    /// Parse one operation per line, with quoted form names and `#` comments.
    /// Bounds and transforms are checked before gameplay can change.
    ///
    /// # Errors
    /// Reports the source line for unknown operations, wrong arity, empty names,
    /// invalid numbers, non-finite transforms or time bounds over 36,000 ticks.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut route = Self::default();
        for (index, line) in text.lines().enumerate() {
            let line_number = index + 1;
            let parse = || -> Result<Option<Step>, String> {
                let words = words(line)?;
                let fields: Vec<_> = words.iter().map(String::as_str).collect();
                let step = match fields.as_slice() {
                    [] => return Ok(None),
                    ["navigate", target, speed, max_ticks] => Step::Navigate {
                        destination: Destination::Reference((*target).into()),
                        speed: speed_value(speed)?,
                        max_ticks: ticks(max_ticks)?,
                    },
                    ["navigate-marker", cell, x, y, z, speed, max_ticks] => Step::Navigate {
                        destination: Destination::Marker(marker(cell, [x, y, z])?),
                        speed: speed_value(speed)?,
                        max_ticks: ticks(max_ticks)?,
                    },
                    ["activate", target] => Step::Activate((*target).into()),
                    ["talk", speaker, topic] => Step::Talk {
                        speaker: (*speaker).into(),
                        topic: (*topic).into(),
                    },
                    ["wait-stage", quest, stage, max_ticks] => Step::WaitStage {
                        quest: (*quest).into(),
                        stage: number(stage)?,
                        max_ticks: ticks(max_ticks)?,
                    },
                    ["expect-stage", quest, stage] => Step::ExpectStage {
                        quest: (*quest).into(),
                        stage: number(stage)?,
                    },
                    ["expect-position", cell, x, y, z, tolerance] => {
                        let tolerance = finite(tolerance)?;
                        if tolerance < 0.0 {
                            return Err("negative position tolerance".into());
                        }
                        Step::ExpectPosition {
                            marker: marker(cell, [x, y, z])?,
                            tolerance,
                        }
                    }
                    ["expect-item", holder, item, count] => Step::ExpectItem {
                        holder: (*holder).into(),
                        item: (*item).into(),
                        count: number(count)?,
                    },
                    ["expect-no-vm-error"] => Step::ExpectNoVmError,
                    _ => return Err("unknown operation or incorrect number of fields".into()),
                };
                Ok(Some(step))
            };
            if let Some(step) = parse().map_err(|reason| Error::Parse {
                line: line_number,
                reason,
            })? {
                route.steps.push((line_number, step));
            }
        }
        if route.steps.is_empty() {
            return Err(Error::Parse {
                line: 1,
                reason: "empty route".into(),
            });
        }
        Ok(route)
    }
}

fn number<T: std::str::FromStr>(value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid number: {value}"))
}

fn finite(value: &str) -> Result<f32, String> {
    let value: f32 = number(value)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("non-finite number".into())
    }
}

fn speed_value(value: &str) -> Result<f32, String> {
    let value = finite(value)?;
    if value > 0.0 && value <= 600.0 {
        Ok(value)
    } else {
        Err("speed must be greater than zero and at most 600 units/second".into())
    }
}

fn ticks(value: &str) -> Result<u32, String> {
    let value = number(value)?;
    if value <= MAX_TICKS {
        Ok(value)
    } else {
        Err(format!("time bound exceeds {MAX_TICKS} ticks"))
    }
}

fn marker(cell: &str, values: [&str; 3]) -> Result<Marker, String> {
    Ok(Marker {
        cell: cell.into(),
        position: [finite(values[0])?, finite(values[1])?, finite(values[2])?],
    })
}

fn words(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    let mut started = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '#' if !quoted => break,
            ch if ch.is_whitespace() && !quoted => {
                if started {
                    if token.is_empty() {
                        return Err("empty field".into());
                    }
                    out.push(std::mem::take(&mut token));
                    started = false;
                }
            }
            ch => {
                token.push(ch);
                started = true;
            }
        }
    }
    if quoted {
        return Err("unclosed quote".into());
    }
    if started {
        if token.is_empty() {
            return Err("empty field".into());
        }
        out.push(token);
    }
    Ok(out)
}
