// SPDX-License-Identifier: GPL-3.0-or-later

//! The determinism detector: a hash of the whole run-time state after every
//! tick, a log of those hashes, and a comparison that names the first tick
//! and subsystem where two runs of one seed part ways. The hash is the
//! world's ([`World::state_hash`]) and the VM's
//! ([`soe_papyrus::Vm::state_hash`]) folded together. Nothing here fixes a
//! divergence, `docs/validation.md` ("Determinism") keeps the list of known
//! causes.
//!
//! [`World::state_hash`]: crate::World::state_hash

use std::fmt;
use std::path::Path;

use soe_papyrus::vm::hash::{StateHasher, VmHash};

use crate::world::hash::WorldHash;
use crate::Game;

/// One tick's hash with its subsystem parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickHash {
    pub tick: u64,
    pub total: u64,
    pub parts: Vec<(&'static str, u64)>,
}

impl TickHash {
    fn new(tick: u64, world: &WorldHash, vm: &VmHash) -> TickHash {
        let mut parts: Vec<(&'static str, u64)> = Vec::with_capacity(PART_NAMES.len());
        parts.extend(world.parts());
        parts.extend(vm.parts());
        let mut h = StateHasher::new();
        for (_, v) in &parts {
            h.sub(*v);
        }
        TickHash {
            tick,
            total: h.finish(),
            parts,
        }
    }

    /// One line: tick, total, then `name=hash` per part.
    fn write_line(&self, out: &mut String) {
        use fmt::Write as _;
        let _ = write!(out, "{} {:016x}", self.tick, self.total);
        for (name, v) in &self.parts {
            let _ = write!(out, " {name}={v:016x}");
        }
        out.push('\n');
    }

    fn parse_line(line: &str) -> Option<TickHash> {
        let mut fields = line.split_whitespace();
        let tick = fields.next()?.parse().ok()?;
        let total = u64::from_str_radix(fields.next()?, 16).ok()?;
        let mut parts = Vec::new();
        for field in fields {
            let (name, hex) = field.split_once('=')?;
            let name = PART_NAMES.iter().find(|n| **n == name)?;
            parts.push((*name, u64::from_str_radix(hex, 16).ok()?));
        }
        Some(TickHash { tick, total, parts })
    }
}

/// Every part name in log order, the world's then the VM's. The text form
/// only accepts these, so a part added to either hash must be added here,
/// the unit test below checks the world half against `WorldHash::parts`.
const PART_NAMES: [&str; 23] = [
    "world.clock",
    "world.rng",
    "world.objects",
    "world.refs",
    "world.actors",
    "world.quests",
    "world.globals",
    "world.form_lists",
    "world.pending",
    "world.timers",
    "world.output",
    "world.ai",
    "world.forms",
    "world.settings",
    "world.listeners",
    "world.flags",
    "world.scenes",
    "vm.objects",
    "vm.instances",
    "vm.threads",
    "vm.latents",
    "vm.log",
    "vm.natives",
];

/// Where two hash logs first disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Divergence {
    /// Both logs cover the tick and at least one part differs.
    AtTick {
        tick: u64,
        /// Every part that differs at that tick, in part order.
        subsystems: Vec<&'static str>,
    },
    /// One log ended before the other.
    Length { left: usize, right: usize },
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Divergence::AtTick { tick, subsystems } => {
                write!(
                    f,
                    "runs diverge at tick {tick} in {}",
                    subsystems.join(", ")
                )
            }
            Divergence::Length { left, right } => {
                write!(f, "runs differ in length, {left} ticks against {right}")
            }
        }
    }
}

/// Compare two hash logs tick by tick.
///
/// # Errors
///
/// The first tick whose hashes differ, with the subsystems that differ
/// there, or the length mismatch when one log is a prefix of the other.
pub fn compare(left: &[TickHash], right: &[TickHash]) -> Result<(), Divergence> {
    for (a, b) in left.iter().zip(right) {
        if a.total == b.total && a.parts == b.parts {
            continue;
        }
        let subsystems: Vec<&'static str> = a
            .parts
            .iter()
            .zip(&b.parts)
            .filter(|(x, y)| x.1 != y.1)
            .map(|(x, _)| x.0)
            .collect();
        return Err(Divergence::AtTick {
            tick: a.tick,
            subsystems,
        });
    }
    if left.len() != right.len() {
        return Err(Divergence::Length {
            left: left.len(),
            right: right.len(),
        });
    }
    Ok(())
}

/// Serialize a hash log as text, one tick per line.
#[must_use]
pub fn to_text(log: &[TickHash]) -> String {
    let mut out = String::new();
    for t in log {
        t.write_line(&mut out);
    }
    out
}

/// Parse a hash log written by [`to_text`]. A malformed line ends the log
/// there, so a truncated file shows up as a length divergence.
#[must_use]
pub fn from_text(text: &str) -> Vec<TickHash> {
    text.lines().map_while(TickHash::parse_line).collect()
}

/// Write a hash log to the file `SOE_HASH_DUMP` names, if it is set.
///
/// # Errors
///
/// The write error, when the variable is set and the file cannot be written.
pub fn dump_if_requested(log: &[TickHash]) -> std::io::Result<()> {
    match std::env::var_os("SOE_HASH_DUMP") {
        Some(path) => std::fs::write(Path::new(&path), to_text(log)),
        None => Ok(()),
    }
}

impl Game {
    /// Hash the world and the VM as they stand.
    #[must_use]
    pub fn state_hash(&self) -> TickHash {
        let tick = self.hash_log.as_ref().map_or(0, |l| l.len() as u64);
        let world = self.world.borrow().state_hash();
        let vm = self.vm.state_hash();
        TickHash::new(tick, &world, &vm)
    }

    /// Start recording a hash after every [`Game::tick`].
    pub fn record_hashes(&mut self) {
        self.hash_log = Some(Vec::new());
    }

    /// The recorded hashes so far, empty when recording is off.
    #[must_use]
    pub fn hash_log(&self) -> &[TickHash] {
        self.hash_log.as_deref().unwrap_or(&[])
    }

    pub(crate) fn push_tick_hash(&mut self) {
        if self.hash_log.is_none() {
            return;
        }
        let hash = self.state_hash();
        if let Some(log) = self.hash_log.as_mut() {
            log.push(hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(tick: u64, parts: &[u64]) -> TickHash {
        let parts: Vec<(&'static str, u64)> = PART_NAMES
            .iter()
            .copied()
            .zip(parts.iter().copied())
            .collect();
        let mut h = StateHasher::new();
        for (_, v) in &parts {
            h.sub(*v);
        }
        TickHash {
            tick,
            total: h.finish(),
            parts,
        }
    }

    #[test]
    fn the_part_names_match_the_world_hash() {
        let world = crate::World::new(soe_esm::LoadOrder::new()).state_hash();
        let names: Vec<&str> = world.parts().iter().map(|p| p.0).collect();
        assert_eq!(names, &PART_NAMES[..names.len()]);
        let rest = &PART_NAMES[names.len()..];
        assert!(rest.iter().all(|n| n.starts_with("vm.")));
    }

    #[test]
    fn compare_names_the_first_tick_and_subsystem() {
        let base = vec![1u64; PART_NAMES.len()];
        let mut changed = base.clone();
        changed[3] = 2;
        changed[19] = 9;
        let left = vec![tick(0, &base), tick(1, &base), tick(2, &base)];
        let right = vec![tick(0, &base), tick(1, &changed), tick(2, &changed)];
        assert_eq!(
            compare(&left, &right),
            Err(Divergence::AtTick {
                tick: 1,
                subsystems: vec!["world.refs", "vm.threads"],
            })
        );
        assert_eq!(
            compare(&left, &left[..2]),
            Err(Divergence::Length { left: 3, right: 2 })
        );
        assert_eq!(compare(&left, &left), Ok(()));
    }

    #[test]
    fn the_text_form_round_trips() {
        let log = vec![
            tick(0, &[0; PART_NAMES.len()]),
            tick(1, &[u64::MAX; PART_NAMES.len()]),
            tick(2, &[7; PART_NAMES.len()]),
        ];
        let text = to_text(&log);
        assert_eq!(from_text(&text), log);
        let truncated = text.lines().next().unwrap().to_string() + "\ngarbage\n";
        assert_eq!(from_text(&truncated).len(), 1);
    }
}
