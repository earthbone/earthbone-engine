// SPDX-License-Identifier: GPL-3.0-or-later

//! Which distant-terrain LOD blocks to have resident around a focus cell,
//! and a recorded byte budget for them.
//!
//! Skyrim's landscape LOD ships as BTR mesh files, one per worldspace grid
//! block at a LOD level, meant to replace full resolution LAND terrain
//! (`crate::data_source::exterior::terrain_surface`) beyond the cells the
//! streamer keeps loaded. The block sizes, 4, 8, 16 and 32 cells to a side,
//! come from TES5Edit's tree and object LOD generator
//! (`wbLOD.pas`, `LODLevelMin`/`LODLevelMax`), the only LOD block sizes the
//! corpus documents. The file naming,
//! `meshes/terrain/<world editor id>/<world>.<level>.<x>.<y>.btr`, is
//! confirmed against a real shipped file, `meshes\terrain\Vyn\vyn.32.-5.9.btr`
//! in Wrye Bash's Enderal SE vanilla file list
//! (`wrye-bash/Mopy/bash/game/enderalse/vanilla_files.py`), and against
//! TES5Edit's `miWorldspaceFilterLODClick`
//! (`Build/Edit Scripts/Worldspace browser.pas`), which builds a block's LOD
//! texture and mesh from the same base name and swaps the extension. Only
//! the level 32 example is an attested real file name, levels 4, 8 and 16
//! follow the same pattern by construction. No corpus source documents
//! landscape LOD generation itself the way `wbLOD.pas` documents tree and
//! object LOD, so block alignment follows `TwbLodSettings.BlockForCell`
//! (`Core/wbLOD.pas`): a block is named by its south-west cell,
//! `SWCell + trunc((cell - SWCell) / level) * level`, where `SWCell` is the
//! worldspace's anchor from `lodsettings\<world>.lod`. The file name's `x`
//! and `y` are that cell, not a block index: `miWorldspaceSaveAsLODClick`
//! in the same `Worldspace browser.pas` steps `x` and `y` in cells by the
//! level when it names each tile, and every level 4 to 32 block this
//! machine's `Skyrim - Meshes0.bsa` carries under `meshes/terrain/tamriel/`
//! sits on a multiple of its level. `LodSettings` reads that 16 byte file
//! with the Skyrim branch of `TwbLodSettings.LoadFromData`.
//!
//! This module decides which blocks belong resident and tracks their
//! recorded byte cost. Reading a block's BTR file is
//! `DataSource::load_lod_block`, which reports back the bytes its meshes
//! actually built to, so the budget reflects what is loaded rather than an
//! estimate made before the read. `TerrainLodStreamer::step` runs one plan,
//! load, record and evict cycle against a caller's loader, which is what
//! the Godot node's `stream_terrain_lod` and `update_terrain_lod` drive. A
//! catalog selects disjoint distance levels from archived block paths, retaining
//! coarser blocks where no finer files exist. Selection is cached until the
//! focus or retained full-cell set changes. Without a catalog, the streamer
//! requests the configured single level on the worldspace's anchored grid.

use std::collections::{HashMap, HashSet};

use crate::streaming::CellCoord;

/// A distant-terrain LOD block's size, in cells to a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodLevel {
    L4,
    L8,
    L16,
    L32,
}

impl LodLevel {
    pub const fn cells(self) -> i32 {
        match self {
            LodLevel::L4 => 4,
            LodLevel::L8 => 8,
            LodLevel::L16 => 16,
            LodLevel::L32 => 32,
        }
    }

    /// The level whose blocks are `cells` to a side, or `None` for a size
    /// the game does not use.
    pub const fn from_cells(cells: i32) -> Option<Self> {
        match cells {
            4 => Some(LodLevel::L4),
            8 => Some(LodLevel::L8),
            16 => Some(LodLevel::L16),
            32 => Some(LodLevel::L32),
            _ => None,
        }
    }
}

/// One worldspace's LOD block at a level. `x` and `y` are the block's
/// south-west cell, the same numbers the BTR file name carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LodBlockCoord {
    pub level: LodLevel,
    pub x: i32,
    pub y: i32,
}

impl LodBlockCoord {
    /// The block assigned to a cell at `level` on a grid anchored at cell
    /// (0, 0), following `TwbLodSettings.BlockForCell`.
    pub fn containing(cell: CellCoord, level: LodLevel) -> Self {
        Self::containing_from(cell, level, CellCoord::new(0, 0))
    }

    /// The block a cell belongs to at `level` on a grid anchored at
    /// `anchor`, the worldspace's `SWCell`, following
    /// `TwbLodSettings.BlockForCell`. Integer division truncates toward zero,
    /// so cells just south or west of the anchor name the anchor's block.
    pub fn containing_from(cell: CellCoord, level: LodLevel, anchor: CellCoord) -> Self {
        let span = level.cells();
        Self {
            level,
            x: anchor.x + (cell.x - anchor.x) / span * span,
            y: anchor.y + (cell.y - anchor.y) / span * span,
        }
    }

    /// The cell the block's south-west corner starts at.
    pub fn origin_cell(self) -> CellCoord {
        CellCoord::new(self.x, self.y)
    }

    /// Chebyshev cell distance from `from` to the block's nearest cell, 0
    /// when `from` falls inside the block. The metric the load and unload
    /// rings use, matching `CellCoord::ring_distance`.
    pub fn ring_distance(self, from: CellCoord) -> i32 {
        let span = i64::from(self.level.cells());
        let min_x = i64::from(self.x);
        let min_y = i64::from(self.y);
        let max_x = min_x + span - 1;
        let max_y = min_y + span - 1;
        let x = i64::from(from.x);
        let y = i64::from(from.y);
        let dx = (min_x - x).max(x - max_x).max(0);
        let dy = (min_y - y).max(y - max_y).max(0);
        dx.max(dy).min(i64::from(i32::MAX)) as i32
    }
}

/// The file's base name, `<world lowercase>.<level>.<x>.<y>`, matching the
/// texture and mesh names TES5Edit's LOD tile filter pairs.
fn btr_stem(world_editor_id: &str, block: LodBlockCoord) -> String {
    format!(
        "{}.{}.{}.{}",
        world_editor_id.to_ascii_lowercase(),
        block.level.cells(),
        block.x,
        block.y
    )
}

/// The BTR file path for one worldspace LOD block. See the module doc for
/// the two corpus sources this convention is read from.
pub fn btr_path(world_editor_id: &str, block: LodBlockCoord) -> String {
    format!(
        "meshes/terrain/{world_editor_id}/{}.btr",
        btr_stem(world_editor_id, block)
    )
}

/// The BTO file path for one worldspace object LOD block. TES5Edit writes
/// object LOD meshes below `Objects` with the same block stem as BTR files.
pub fn bto_path(world_editor_id: &str, block: LodBlockCoord) -> String {
    format!(
        "meshes/terrain/{world_editor_id}/Objects/{}.bto",
        btr_stem(world_editor_id, block)
    )
}

/// The BTT tree-reference path, from xEdit TwbLodTES5TreeBlock.
pub fn btt_path(world_editor_id: &str, block: LodBlockCoord) -> String {
    format!(
        "meshes/terrain/{world_editor_id}/Trees/{}.btt",
        btr_stem(world_editor_id, block)
    )
}

/// Archived scenery to stream through the load-order VFS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LodKind {
    Terrain,
    Object,
    Tree,
}

/// Select disjoint archived blocks, refining coarse blocks near the focus
/// where finer files exist. Near-cell overlap always descends or disappears.
/// Empty portions of a finer level have no archived geometry to request.
pub fn select_lod_blocks(
    catalog: &[LodBlockCoord],
    focus: CellCoord,
    full_res_radius: i32,
    lod_radius: i32,
) -> Vec<LodBlockCoord> {
    select_lod_blocks_excluding(catalog, focus, full_res_radius, lod_radius, &[])
}

fn select_lod_blocks_excluding(
    catalog: &[LodBlockCoord],
    focus: CellCoord,
    full_res_radius: i32,
    lod_radius: i32,
    full_cells: &[CellCoord],
) -> Vec<LodBlockCoord> {
    let mut blocks = catalog.to_vec();
    blocks.sort_unstable_by_key(|b| (std::cmp::Reverse(b.level.cells()), b.x, b.y));
    blocks.dedup();
    let contains = |outer: LodBlockCoord, inner: LodBlockCoord| {
        inner.x >= outer.x
            && inner.y >= outer.y
            && i64::from(inner.x) + i64::from(inner.level.cells())
                <= i64::from(outer.x) + i64::from(outer.level.cells())
            && i64::from(inner.y) + i64::from(inner.level.cells())
                <= i64::from(outer.y) + i64::from(outer.level.cells())
    };
    let mut selected: Vec<LodBlockCoord> = Vec::new();
    for block in &blocks {
        let distance = block.ring_distance(focus);
        if distance > lod_radius || selected.iter().any(|parent| contains(*parent, *block)) {
            continue;
        }
        let full_overlap = full_cells
            .iter()
            .any(|cell| block.ring_distance(*cell) == 0);
        let refine = (distance <= block.level.cells() * 2 || full_overlap)
            && blocks
                .iter()
                .any(|child| child.level.cells() < block.level.cells() && contains(*block, *child));
        if distance > full_res_radius && !full_overlap && !refine {
            selected.push(*block);
        }
    }
    selected.sort_unstable_by_key(|b| (b.ring_distance(focus), b.level.cells(), b.x, b.y));
    selected
}

/// The worldspace's LOD settings file, `TwbLodSettings.GetFileName` for
/// Skyrim.
pub fn lodsettings_path(world_editor_id: &str) -> String {
    format!("lodsettings/{world_editor_id}.lod")
}

/// A worldspace's `lodsettings\<world>.lod`: the anchor cell every LOD
/// block is aligned from, and the block sizes the worldspace was baked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodSettings {
    pub sw_cell: CellCoord,
    pub stride: i32,
    pub level_min: i32,
    pub level_max: i32,
}

impl LodSettings {
    /// The Skyrim layout from `TwbLodSettings.LoadFromData`: two `i16`
    /// for `SWCell`, then `i32` stride, minimum and maximum level, 16 bytes
    /// in all, little endian. Any other length is not a Skyrim file.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; 16] = bytes.try_into().ok()?;
        let i16_at = |at: usize| i32::from(i16::from_le_bytes([bytes[at], bytes[at + 1]]));
        let i32_at = |at: usize| {
            i32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        Some(Self {
            sw_cell: CellCoord::new(i16_at(0), i16_at(2)),
            stride: i32_at(4),
            level_min: i32_at(8),
            level_max: i32_at(12),
        })
    }

    /// The finest level the worldspace was baked at, the one that stands
    /// in for terrain nearest the loaded cells.
    pub fn finest_level(&self) -> Option<LodLevel> {
        LodLevel::from_cells(self.level_min)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LodConfigError {
    #[error("full resolution radius {0} is negative")]
    NegativeRadius(i32),
    #[error("lod radius {lod} does not exceed the full resolution radius {full_res}")]
    LodInsideFullRes { full_res: i32, lod: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLodConfig {
    pub level: LodLevel,
    /// Ring distance in cells, inclusive, that keeps full resolution LAND
    /// terrain. Cover the cell streamer's retained cells, including its
    /// unload radius when it uses hysteresis.
    pub full_res_radius: i32,
    /// Ring distance in cells, inclusive, out to which LOD blocks replace
    /// full resolution terrain. Must exceed `full_res_radius`.
    pub lod_radius: i32,
    /// Most blocks to start loading in one `update`. 0 means unlimited.
    pub max_loads_per_tick: usize,
    /// Most distant blocks to release in one `update`. 0 means unlimited.
    /// Blocks overlapping full resolution coverage are always released immediately.
    pub max_unloads_per_tick: usize,
    /// Recorded byte cap across resident blocks. 0 means unlimited, and
    /// `evict_to_budget` is then a no-op.
    pub budget_bytes: u64,
}

impl Default for TerrainLodConfig {
    fn default() -> Self {
        Self {
            level: LodLevel::L4,
            full_res_radius: 2,
            lod_radius: 8,
            max_loads_per_tick: 1,
            max_unloads_per_tick: 2,
            budget_bytes: 0,
        }
    }
}

impl TerrainLodConfig {
    pub fn validate(&self) -> Result<(), LodConfigError> {
        if self.full_res_radius < 0 {
            return Err(LodConfigError::NegativeRadius(self.full_res_radius));
        }
        if self.lod_radius <= self.full_res_radius {
            return Err(LodConfigError::LodInsideFullRes {
                full_res: self.full_res_radius,
                lod: self.lod_radius,
            });
        }
        Ok(())
    }
}

/// What one `update` asks the caller to do. Loads are nearest first, unloads
/// farthest first, matching `streaming::StreamPlan`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TerrainLodPlan {
    pub load: Vec<LodBlockCoord>,
    pub unload: Vec<LodBlockCoord>,
}

impl TerrainLodPlan {
    pub fn is_empty(&self) -> bool {
        self.load.is_empty() && self.unload.is_empty()
    }
}

/// What one `TerrainLodStreamer::step` did: the blocks its loader built,
/// with their recorded bytes, the blocks it found no BTR file for, the
/// blocks whose read failed, and every block it released (planned unloads
/// then budget evictions), which the caller frees.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LodStep {
    pub loaded: Vec<(LodBlockCoord, u64)>,
    pub missing: Vec<LodBlockCoord>,
    pub failed: Vec<(LodBlockCoord, String)>,
    pub unloaded: Vec<LodBlockCoord>,
    pub evicted: Vec<LodBlockCoord>,
}

impl LodStep {
    /// Every block the caller must free, unloads then evictions.
    pub fn released(&self) -> impl Iterator<Item = LodBlockCoord> + '_ {
        self.unloaded.iter().chain(&self.evicted).copied()
    }
}

/// Decides which LOD blocks stand in for full resolution terrain around a
/// focus cell, and tracks the recorded byte cost of the blocks currently
/// resident. Mirrors `streaming::CellStreamer`'s hysteresis at block
/// granularity, with a budget the cell streamer has no equivalent of.
#[derive(Debug, Clone)]
pub struct TerrainLodStreamer {
    config: TerrainLodConfig,
    /// The worldspace's `SWCell`, which the block grid is aligned from.
    anchor: CellCoord,
    /// Blocks requested and not yet asked to unload, with the bytes
    /// `record_bytes` reported for them. 0 until a caller records a real
    /// reading, so a block requested this tick and not yet loaded reads as
    /// weightless rather than wrongly full.
    resident: HashMap<LodBlockCoord, u64>,
    /// Blocks the data has no BTR file for. Absent content stays absent for
    /// the data source's lifetime, so `update` never asks for them again
    /// and a missing block cannot take a per-tick load slot every frame.
    absent: HashSet<LodBlockCoord>,
    /// Blocks `evict_to_budget` released, held back from `update` until the
    /// focus moves to another cell. Without this a block over budget is
    /// loaded, evicted and read again on every tick at the same focus.
    held_back: HashSet<LodBlockCoord>,
    /// The focus of the last `update`, which decides when `held_back` clears.
    last_focus: Option<CellCoord>,
    catalog: Option<Vec<LodBlockCoord>>,
    selected: Option<Vec<LodBlockCoord>>,
    full_cells: Vec<CellCoord>,
}

impl TerrainLodStreamer {
    pub fn new(config: TerrainLodConfig) -> Result<Self, LodConfigError> {
        Self::with_anchor(config, CellCoord::new(0, 0))
    }

    /// A streamer whose block grid is aligned from `anchor`, the
    /// worldspace's `LodSettings::sw_cell`.
    pub fn with_anchor(
        config: TerrainLodConfig,
        anchor: CellCoord,
    ) -> Result<Self, LodConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            anchor,
            resident: HashMap::new(),
            absent: HashSet::new(),
            held_back: HashSet::new(),
            last_focus: None,
            catalog: None,
            selected: None,
            full_cells: Vec::new(),
        })
    }

    /// Restrict requests to archived blocks and choose their distance levels.
    pub fn with_catalog(
        config: TerrainLodConfig,
        anchor: CellCoord,
        catalog: Vec<LodBlockCoord>,
    ) -> Result<Self, LodConfigError> {
        let mut streamer = Self::with_anchor(config, anchor)?;
        streamer.catalog = Some(catalog);
        Ok(streamer)
    }

    /// Exclude retained full cells from archived selection outside the near ring.
    /// Changing this set invalidates the cached distance selection.
    pub fn set_full_cells(&mut self, cells: &[CellCoord]) {
        let mut cells = cells.to_vec();
        cells.sort_unstable_by_key(|cell| (cell.x, cell.y));
        cells.dedup();
        if self.full_cells != cells {
            self.full_cells = cells;
            self.selected = None;
        }
    }

    pub fn config(&self) -> &TerrainLodConfig {
        &self.config
    }

    pub fn anchor(&self) -> CellCoord {
        self.anchor
    }

    /// The caller found no BTR file for `block`. It stops being resident
    /// and is never requested again.
    pub fn mark_absent(&mut self, block: LodBlockCoord) {
        self.resident.remove(&block);
        self.absent.insert(block);
    }

    /// Blocks known to have no BTR file.
    pub fn absent_count(&self) -> usize {
        self.absent.len()
    }

    pub fn resident_blocks(&self) -> impl Iterator<Item = LodBlockCoord> + '_ {
        self.resident.keys().copied()
    }

    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }

    pub fn is_resident(&self, block: LodBlockCoord) -> bool {
        self.resident.contains_key(&block)
    }

    /// Recorded bytes across every resident block, the residency budget's
    /// current reading.
    pub fn usage_bytes(&self) -> u64 {
        self.resident.values().sum()
    }

    pub fn over_budget(&self) -> bool {
        self.config.budget_bytes > 0 && self.usage_bytes() > self.config.budget_bytes
    }

    /// The caller failed to load, or dropped, a block on its own.
    pub fn mark_unloaded(&mut self, block: LodBlockCoord) {
        self.resident.remove(&block);
    }

    /// Records the bytes a just-loaded block's meshes actually built to,
    /// from `DataSource::load_lod_block`'s `approx_bytes`. Overwrites any
    /// earlier reading for the same block.
    pub fn record_bytes(&mut self, block: LodBlockCoord, bytes: u64) {
        self.resident.insert(block, bytes);
    }

    /// Decide loads and unloads for a focus cell: which LOD blocks should
    /// replace full resolution terrain beyond `full_res_radius`, out to
    /// `lod_radius`. Cheap enough to call every tick, quadratic in
    /// `lod_radius / level.cells()`.
    pub fn update(&mut self, focus: CellCoord) -> TerrainLodPlan {
        if self.last_focus != Some(focus) {
            self.held_back.clear();
            self.last_focus = Some(focus);
            self.selected = None;
        }
        let level = self.config.level;
        let span = level.cells();
        let r = self.config.lod_radius;
        let low = LodBlockCoord::containing_from(
            CellCoord::new(focus.x - r, focus.y - r),
            level,
            self.anchor,
        );
        let high = LodBlockCoord::containing_from(
            CellCoord::new(focus.x + r, focus.y + r),
            level,
            self.anchor,
        );

        let mut wanted: Vec<LodBlockCoord> = Vec::new();
        for y in (low.y..=high.y).step_by(span as usize) {
            for x in (low.x..=high.x).step_by(span as usize) {
                let block = LodBlockCoord { level, x, y };
                let d = block.ring_distance(focus);
                if d > self.config.full_res_radius
                    && d <= r
                    && !self.resident.contains_key(&block)
                    && !self.absent.contains(&block)
                    && !self.held_back.contains(&block)
                {
                    wanted.push(block);
                }
            }
        }
        if self.selected.is_none() {
            self.selected = self.catalog.as_ref().map(|catalog| {
                select_lod_blocks_excluding(
                    catalog,
                    focus,
                    self.config.full_res_radius,
                    r,
                    &self.full_cells,
                )
            });
        }
        let selected = &self.selected;
        if let Some(selected) = selected {
            wanted = selected
                .iter()
                .copied()
                .filter(|block| {
                    !self.resident.contains_key(block)
                        && !self.absent.contains(block)
                        && !self.held_back.contains(block)
                })
                .collect();
        }
        wanted.sort_by_key(|b| (b.ring_distance(focus), b.level.cells(), b.x, b.y));
        if self.config.max_loads_per_tick > 0 {
            wanted.truncate(self.config.max_loads_per_tick);
        }

        let mut stale: Vec<LodBlockCoord> = self
            .resident
            .keys()
            .copied()
            .filter(|b| {
                let d = b.ring_distance(focus);
                d > r
                    || d <= self.config.full_res_radius
                    || selected.as_ref().is_some_and(|blocks| !blocks.contains(b))
            })
            .collect();
        stale.sort_by_key(|b| (std::cmp::Reverse(b.ring_distance(focus)), b.x, b.y));
        let mut distant_unloads = 0;
        stale.retain(|block| {
            if block.ring_distance(focus) <= self.config.full_res_radius
                || selected
                    .as_ref()
                    .is_some_and(|blocks| !blocks.contains(block))
            {
                return true;
            }
            distant_unloads += 1;
            self.config.max_unloads_per_tick == 0
                || distant_unloads <= self.config.max_unloads_per_tick
        });

        for b in &wanted {
            self.resident.entry(*b).or_insert(0);
        }
        for b in &stale {
            self.resident.remove(b);
        }
        TerrainLodPlan {
            load: wanted,
            unload: stale,
        }
    }

    /// Run `update` until it has nothing left to ask for the same focus.
    /// For loading screens and tests, where spreading work over frames is
    /// not wanted.
    pub fn settle(&mut self, focus: CellCoord) -> TerrainLodPlan {
        let mut total = TerrainLodPlan::default();
        loop {
            let plan = self.update(focus);
            if plan.is_empty() {
                return total;
            }
            total.load.extend(plan.load);
            total.unload.extend(plan.unload);
        }
    }

    /// When resident bytes exceed the budget, release the farthest blocks
    /// from `focus` until usage fits again, and return what was released.
    /// This is a hard cap, not a smooth degrade. An evicted block is held
    /// back from `update` until the focus moves to another cell, so a
    /// block that does not fit is not read again on every tick at the same
    /// focus. Raising `budget_bytes` or shrinking `lod_radius` is the fix
    /// for sustained pressure, this only enforces the cap.
    pub fn evict_to_budget(&mut self, focus: CellCoord) -> Vec<LodBlockCoord> {
        if self.config.budget_bytes == 0 {
            return Vec::new();
        }
        let mut evicted = Vec::new();
        while self.usage_bytes() > self.config.budget_bytes {
            let farthest = self
                .resident
                .keys()
                .copied()
                .max_by_key(|b| (b.ring_distance(focus), b.x, b.y));
            let Some(block) = farthest else {
                break;
            };
            self.resident.remove(&block);
            self.held_back.insert(block);
            evicted.push(block);
        }
        evicted
    }

    /// One full cycle for `focus`: plan (`settle` when `settle` is set,
    /// otherwise one capped `update`), hand each planned load to `load`,
    /// record what it built, then enforce the budget. `load` returns the
    /// block's built bytes, `None` when the data has no BTR file for it, or
    /// an error. A missing block is marked absent, a failed one is dropped
    /// from residency and may be asked for again on a later tick. The
    /// caller frees every block in `LodStep::released` and keeps the rest.
    pub fn step<E: std::fmt::Display>(
        &mut self,
        focus: CellCoord,
        settle: bool,
        mut load: impl FnMut(LodBlockCoord) -> Result<Option<u64>, E>,
    ) -> LodStep {
        let plan = if settle {
            self.settle(focus)
        } else {
            self.update(focus)
        };
        let mut step = LodStep {
            unloaded: plan.unload,
            ..LodStep::default()
        };
        for block in plan.load {
            match load(block) {
                Ok(Some(bytes)) => {
                    self.record_bytes(block, bytes);
                    step.loaded.push((block, bytes));
                }
                Ok(None) => {
                    self.mark_absent(block);
                    step.missing.push(block);
                }
                Err(e) => {
                    self.mark_unloaded(block);
                    step.failed.push((block, e.to_string()));
                }
            }
        }
        step.evicted = self.evict_to_budget(focus);
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(budget_bytes: u64) -> TerrainLodConfig {
        TerrainLodConfig {
            level: LodLevel::L4,
            full_res_radius: 2,
            lod_radius: 10,
            max_loads_per_tick: 0,
            max_unloads_per_tick: 0,
            budget_bytes,
        }
    }

    /// The file name carries the block's south-west cell. Level 4 block
    /// (-68, -36) is a real Tamriel file on a vanilla install.
    #[test]
    fn btr_path_names_the_south_west_cell() {
        let block = LodBlockCoord::containing_from(
            CellCoord::new(-66, -35),
            LodLevel::L4,
            CellCoord::new(-96, -64),
        );
        assert_eq!(block.origin_cell(), CellCoord::new(-68, -36));
        assert_eq!(
            btr_path("Tamriel", block),
            "meshes/terrain/Tamriel/tamriel.4.-68.-36.btr"
        );
    }

    /// Enderal's `vyn.32.-5.9.btr` sits off a multiple of 32, which only an
    /// anchor at `SWCell` explains.
    #[test]
    fn an_anchor_shifts_the_block_grid() {
        let anchor = CellCoord::new(-5, 9);
        let block = LodBlockCoord::containing_from(CellCoord::new(10, 20), LodLevel::L32, anchor);
        assert_eq!((block.x, block.y), (-5, 9));
        let west = LodBlockCoord::containing_from(CellCoord::new(-6, 8), LodLevel::L32, anchor);
        assert_eq!((west.x, west.y), (-5, 9));
    }

    #[test]
    fn lodsettings_reads_the_skyrim_layout() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(-96i16).to_le_bytes());
        bytes.extend_from_slice(&(-64i16).to_le_bytes());
        bytes.extend_from_slice(&4096i32.to_le_bytes());
        bytes.extend_from_slice(&4i32.to_le_bytes());
        bytes.extend_from_slice(&32i32.to_le_bytes());
        let settings = LodSettings::parse(&bytes).unwrap_or_else(|| panic!("16 bytes parse"));
        assert_eq!(settings.sw_cell, CellCoord::new(-96, -64));
        assert_eq!(settings.stride, 4096);
        assert_eq!(settings.finest_level(), Some(LodLevel::L4));
        assert_eq!(LodSettings::parse(&bytes[..15]), None);
    }

    /// Every block `update` plans lies on the anchored grid.
    #[test]
    fn planned_blocks_sit_on_the_anchored_grid() {
        let anchor = CellCoord::new(-5, 9);
        let mut s = TerrainLodStreamer::with_anchor(config(0), anchor)
            .unwrap_or_else(|e| panic!("with_anchor: {e}"));
        let plan = s.settle(CellCoord::new(3, 3));
        assert_ne!(plan.load.len(), 0);
        for b in &plan.load {
            assert_eq!((b.x - anchor.x).rem_euclid(4), 0, "{b:?}");
            assert_eq!((b.y - anchor.y).rem_euclid(4), 0, "{b:?}");
        }
    }

    /// With one load a tick, a missing nearest block must not take the slot
    /// again on the next tick, or the streamer never reaches the others.
    #[test]
    fn a_missing_block_is_not_requested_twice() {
        let mut s = TerrainLodStreamer::new(TerrainLodConfig {
            max_loads_per_tick: 1,
            ..config(0)
        })
        .unwrap_or_else(|e| panic!("new: {e}"));
        let focus = CellCoord::new(0, 0);
        let first = s.step(focus, false, |_| Ok::<_, String>(None));
        assert_eq!(first.missing.len(), 1);
        let second = s.step(focus, false, |_| Ok::<_, String>(Some(10)));
        assert_eq!(second.loaded.len(), 1);
        assert_ne!(second.loaded[0].0, first.missing[0]);
        assert_eq!(s.absent_count(), 1);
    }

    /// An evicted block stays out until the focus moves, then comes back.
    #[test]
    fn an_evicted_block_is_held_back_until_the_focus_moves() {
        let mut s = TerrainLodStreamer::new(config(25)).unwrap_or_else(|e| panic!("new: {e}"));
        let focus = CellCoord::new(0, 0);
        let first = s.step(focus, true, |_| Ok::<_, String>(Some(10)));
        assert_ne!(first.evicted.len(), 0);
        assert!(s.usage_bytes() <= 25);
        let again = s.step(focus, false, |_| Ok::<_, String>(Some(10)));
        assert!(again.loaded.is_empty(), "{again:?}");
        let moved = s.step(CellCoord::new(1, 0), false, |_| Ok::<_, String>(Some(10)));
        assert_ne!(moved.loaded.len(), 0);
    }

    #[test]
    fn a_failed_read_is_reported_and_not_resident() {
        let mut s = TerrainLodStreamer::new(config(0)).unwrap_or_else(|e| panic!("new: {e}"));
        let step = s.step(CellCoord::new(0, 0), true, |_| Err("bad block"));
        assert_ne!(step.failed.len(), 0);
        assert_eq!(s.resident_count(), 0);
        assert_eq!(s.absent_count(), 0);
    }
}
