// SPDX-License-Identifier: GPL-3.0-or-later

//! Which exterior cells to have loaded around a focus, and in what order.
//!
//! The game keeps a square of `uGridsToLoad` cells (5 by default) around the
//! player and swaps rows as the player crosses a cell boundary. This keeps
//! the same shape but adds two things the original lacks: a wider unload
//! radius than load radius so a player pacing on a boundary does not thrash,
//! and a per-tick budget so a fast traversal spreads work over frames
//! instead of stalling one. The streamer only decides. Loading is the
//! caller's, through the plan it returns.

use std::collections::HashSet;

use crate::units::{SkyrimPos, CELL_UNITS};

/// An exterior cell in a worldspace's grid. Cell (0, 0) spans units 0 to
/// 4095 on both axes, negative cells go west and south.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// The cell containing a world position.
    pub fn containing(pos: SkyrimPos) -> Self {
        Self {
            x: (pos.x / CELL_UNITS).floor() as i32,
            y: (pos.y / CELL_UNITS).floor() as i32,
        }
    }

    /// Chebyshev distance in cells, the metric the load square uses.
    pub fn ring_distance(self, other: CellCoord) -> i32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }

    /// Squared Euclidean distance in cells, for ordering within a ring.
    pub fn distance_sq(self, other: CellCoord) -> i64 {
        let dx = (self.x - other.x) as i64;
        let dy = (self.y - other.y) as i64;
        dx * dx + dy * dy
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamConfig {
    /// Cells on each side of the focus cell to keep loaded. 2 is the game's
    /// `uGridsToLoad=5`.
    pub load_radius: i32,
    /// Cells on each side beyond which a loaded cell is released. Must be at
    /// least `load_radius`; the gap is the hysteresis band.
    pub unload_radius: i32,
    /// Most cells to start loading in one `update`. 0 means unlimited.
    pub max_loads_per_tick: usize,
    /// Most cells to release in one `update`. 0 means unlimited.
    pub max_unloads_per_tick: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            load_radius: 2,
            unload_radius: 3,
            max_loads_per_tick: 2,
            max_unloads_per_tick: 4,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ConfigError {
    #[error("load radius {0} is negative")]
    NegativeRadius(i32),
    #[error("unload radius {unload} is smaller than load radius {load}")]
    UnloadInsideLoad { load: i32, unload: i32 },
}

impl StreamConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.load_radius < 0 {
            return Err(ConfigError::NegativeRadius(self.load_radius));
        }
        if self.unload_radius < self.load_radius {
            return Err(ConfigError::UnloadInsideLoad {
                load: self.load_radius,
                unload: self.unload_radius,
            });
        }
        Ok(())
    }
}

/// What one `update` asks the caller to do. Loads are nearest first, unloads
/// farthest first, so a truncated budget still does the most useful work.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StreamPlan {
    pub load: Vec<CellCoord>,
    pub unload: Vec<CellCoord>,
}

impl StreamPlan {
    pub fn is_empty(&self) -> bool {
        self.load.is_empty() && self.unload.is_empty()
    }
}

#[derive(Debug)]
pub struct CellStreamer {
    config: StreamConfig,
    /// Cells the caller has been asked to load and not yet asked to unload.
    /// The streamer assumes a requested load lands, so a load that failed
    /// must be reported back through `mark_unloaded` to be retried.
    resident: HashSet<CellCoord>,
    last_focus: Option<CellCoord>,
}

impl CellStreamer {
    pub fn new(config: StreamConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            resident: HashSet::new(),
            last_focus: None,
        })
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn resident(&self) -> impl Iterator<Item = CellCoord> + '_ {
        self.resident.iter().copied()
    }

    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }

    pub fn is_resident(&self, cell: CellCoord) -> bool {
        self.resident.contains(&cell)
    }

    /// The caller failed to load, or dropped, a cell on its own.
    pub fn mark_unloaded(&mut self, cell: CellCoord) {
        self.resident.remove(&cell);
    }

    /// Decide loads and unloads for a focus position. Cheap enough to call
    /// every frame: it is quadratic in the load radius, which is tiny.
    pub fn update(&mut self, focus: SkyrimPos) -> StreamPlan {
        let centre = CellCoord::containing(focus);
        self.last_focus = Some(centre);

        let r = self.config.load_radius;
        let mut wanted: Vec<CellCoord> = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
        for y in (centre.y - r)..=(centre.y + r) {
            for x in (centre.x - r)..=(centre.x + r) {
                let c = CellCoord::new(x, y);
                if !self.resident.contains(&c) {
                    wanted.push(c);
                }
            }
        }
        // Nearest first. The tie-break on coordinates keeps the order stable
        // across runs, which the tests and the benchmark rely on.
        wanted.sort_by_key(|c| (c.distance_sq(centre), c.x, c.y));
        if self.config.max_loads_per_tick > 0 {
            wanted.truncate(self.config.max_loads_per_tick);
        }

        let ur = self.config.unload_radius;
        let mut stale: Vec<CellCoord> = self
            .resident
            .iter()
            .copied()
            .filter(|c| c.ring_distance(centre) > ur)
            .collect();
        stale.sort_by_key(|c| (std::cmp::Reverse(c.distance_sq(centre)), c.x, c.y));
        if self.config.max_unloads_per_tick > 0 {
            stale.truncate(self.config.max_unloads_per_tick);
        }

        for c in &wanted {
            self.resident.insert(*c);
        }
        for c in &stale {
            self.resident.remove(c);
        }
        StreamPlan {
            load: wanted,
            unload: stale,
        }
    }

    /// Run `update` until it has nothing left to ask for the same focus.
    /// For loading screens and tests, where spreading work over frames is
    /// not wanted.
    pub fn settle(&mut self, focus: SkyrimPos) -> StreamPlan {
        let mut total = StreamPlan::default();
        loop {
            let plan = self.update(focus);
            if plan.is_empty() {
                return total;
            }
            total.load.extend(plan.load);
            total.unload.extend(plan.unload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlimited(load: i32, unload: i32) -> CellStreamer {
        CellStreamer::new(StreamConfig {
            load_radius: load,
            unload_radius: unload,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 0,
        })
        .unwrap()
    }

    #[test]
    fn cell_of_a_position_floors_toward_negative() {
        assert_eq!(CellCoord::containing(SkyrimPos::new(0.0, 0.0, 0.0)), CellCoord::new(0, 0));
        assert_eq!(CellCoord::containing(SkyrimPos::new(4095.9, 4096.0, 0.0)), CellCoord::new(0, 1));
        assert_eq!(CellCoord::containing(SkyrimPos::new(-0.5, -4096.5, 0.0)), CellCoord::new(-1, -2));
    }

    #[test]
    fn a_fresh_streamer_loads_the_full_square_nearest_first() {
        let mut s = unlimited(2, 3);
        let plan = s.update(SkyrimPos::new(100.0, 100.0, 0.0));
        assert_eq!(plan.load.len(), 25);
        assert!(plan.unload.is_empty());
        assert_eq!(plan.load[0], CellCoord::new(0, 0));
        // The four edge-adjacent cells come before any diagonal.
        for c in &plan.load[1..5] {
            assert_eq!(c.distance_sq(CellCoord::new(0, 0)), 1, "{c:?}");
        }
        assert_eq!(s.resident_count(), 25);
    }

    #[test]
    fn the_budget_caps_loads_per_tick_and_settle_drains_it() {
        let mut s = CellStreamer::new(StreamConfig {
            max_loads_per_tick: 3,
            ..StreamConfig::default()
        })
        .unwrap();
        let first = s.update(SkyrimPos::default());
        assert_eq!(first.load.len(), 3);
        let rest = s.settle(SkyrimPos::default());
        assert_eq!(rest.load.len(), 22);
        assert!(s.update(SkyrimPos::default()).is_empty());
    }

    #[test]
    fn crossing_one_boundary_loads_a_column_and_unloads_nothing_inside_hysteresis() {
        let mut s = unlimited(2, 3);
        s.settle(SkyrimPos::default());
        // One cell east: new column x=3 wanted, column x=-2 is now at ring 3,
        // inside the unload radius, so it stays.
        let plan = s.update(SkyrimPos::new(CELL_UNITS + 1.0, 0.0, 0.0));
        assert_eq!(plan.load.len(), 5);
        assert!(plan.load.iter().all(|c| c.x == 3));
        assert!(plan.unload.is_empty());
        assert_eq!(s.resident_count(), 30);
        // Two cells east: column x=-2 is at ring 4 and goes.
        let plan = s.update(SkyrimPos::new(2.0 * CELL_UNITS + 1.0, 0.0, 0.0));
        assert_eq!(plan.unload.len(), 5);
        assert!(plan.unload.iter().all(|c| c.x == -2));
        assert_eq!(s.resident_count(), 30);
    }

    #[test]
    fn pacing_on_a_boundary_does_not_thrash() {
        let mut s = unlimited(2, 3);
        s.settle(SkyrimPos::default());
        let mut unloads = 0;
        for i in 0..20 {
            let x = if i % 2 == 0 { -1.0 } else { 1.0 };
            unloads += s.update(SkyrimPos::new(x, 0.0, 0.0)).unload.len();
        }
        assert_eq!(unloads, 0);
        assert_eq!(s.resident_count(), 30);
    }

    #[test]
    fn unloads_go_farthest_first_under_a_budget() {
        let mut s = CellStreamer::new(StreamConfig {
            load_radius: 1,
            unload_radius: 1,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 1,
        })
        .unwrap();
        s.settle(SkyrimPos::default());
        let far = SkyrimPos::new(10.0 * CELL_UNITS, 10.0 * CELL_UNITS, 0.0);
        let plan = s.update(far);
        assert_eq!(plan.unload, vec![CellCoord::new(-1, -1)]);
    }

    #[test]
    fn a_failed_load_is_retried_after_mark_unloaded() {
        let mut s = unlimited(0, 0);
        assert_eq!(s.update(SkyrimPos::default()).load, vec![CellCoord::new(0, 0)]);
        assert!(s.update(SkyrimPos::default()).load.is_empty());
        s.mark_unloaded(CellCoord::new(0, 0));
        assert_eq!(s.update(SkyrimPos::default()).load, vec![CellCoord::new(0, 0)]);
    }

    #[test]
    fn bad_configs_are_rejected() {
        let bad = StreamConfig {
            load_radius: 3,
            unload_radius: 2,
            ..StreamConfig::default()
        };
        assert_eq!(
            CellStreamer::new(bad).err(),
            Some(ConfigError::UnloadInsideLoad { load: 3, unload: 2 })
        );
        let neg = StreamConfig {
            load_radius: -1,
            ..StreamConfig::default()
        };
        assert_eq!(CellStreamer::new(neg).err(), Some(ConfigError::NegativeRadius(-1)));
    }
}
