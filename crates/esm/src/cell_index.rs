// SPDX-License-Identifier: GPL-3.0-or-later

//! An index from cells to what they hold, built once over the load order.
//!
//! Every placed object (`REFR`, `ACHR`, the projectile and hazard types)
//! sits in a cell's persistent or temporary children group, every `LAND`
//! and `NAVM` in a temporary group, and every exterior cell under its
//! worldspace with an `XCLC` grid position. `LoadOrder` keeps that placement
//! on each record but cannot answer "what is in this cell" short of a pass
//! over the whole table, which `host.scan_references` measured at 361 ms
//! per cell. The index turns the question into a hash lookup.
//!
//! Override rules follow the form table. The cell a placed object belongs
//! to is read from its winning record's placement, so a plugin that moves a
//! reference into another cell moves it here too. A placed object whose
//! winner carries the deleted flag is kept under its last known cell in a
//! separate list, `deleted_references`, so a script or a linked reference
//! that still names it resolves and the game starts it disabled, which is
//! what xEdit's undelete-and-disable cleaning would have produced. The hot
//! `references` slice never lists it. A deleted `LAND` or `NAVM` winner is
//! left out, the navmesh crate repairs the edge links that pointed at it.
//! Cells are keyed by resolved id, exterior cells also by resolved
//! worldspace id plus grid position.
//!
//! A worldspace's persistent references live in one cell placed directly
//! under the `WRLD` children group rather than in any grid cell. The index
//! keeps that cell per worldspace so an exterior loader can add them, it
//! does not spread them over the grid.

use std::collections::HashMap;

use crate::load_order::{GlobalFormId, LoadOrder};
use crate::plugin::{GroupKind, Record};
use crate::tag::{self, Tag};

/// Record types filed under a cell as placed objects.
pub const PLACED_TAGS: [Tag; 10] = [
    tag::REFR,
    tag::ACHR,
    tag::PGRE,
    tag::PHZD,
    tag::PMIS,
    tag::PARW,
    tag::PBEA,
    tag::PFLA,
    tag::PCON,
    tag::PBAR,
];

/// An exterior cell's place in its worldspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridCell {
    pub world: GlobalFormId,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Default)]
struct CellEntry {
    /// Persistent placed objects first, temporary ones after, each part
    /// sorted by id so a walk over a cell is deterministic.
    placed: Vec<GlobalFormId>,
    persistent: usize,
    /// Placed objects whose winner is deleted, sorted by id. Kept apart so
    /// `placed` stays what a loader draws.
    deleted: Vec<GlobalFormId>,
    land: Option<GlobalFormId>,
    navmeshes: Vec<GlobalFormId>,
    interior: bool,
}

#[derive(Debug, Default)]
pub struct CellIndex {
    cells: HashMap<GlobalFormId, CellEntry>,
    grid: HashMap<GridCell, GlobalFormId>,
    grid_of: HashMap<GlobalFormId, GridCell>,
    world_cells: HashMap<GlobalFormId, GlobalFormId>,
    by_editor_id: HashMap<String, GlobalFormId>,
    placed: usize,
    deleted: usize,
}

impl CellIndex {
    /// Walk every winning record once and file it under its cell.
    #[must_use]
    pub fn build(order: &LoadOrder) -> CellIndex {
        let mut span = soe_trace::Span::new("esm.cell_index_build", "esm");
        let mut persistent: HashMap<GlobalFormId, Vec<GlobalFormId>> = HashMap::new();
        let mut temporary: HashMap<GlobalFormId, Vec<GlobalFormId>> = HashMap::new();
        let mut index = CellIndex::default();
        for (id, rref) in order.winners() {
            let rec = order.record(rref);
            if rec.is_deleted() {
                if PLACED_TAGS.contains(&rec.tag) {
                    if let Some(raw_cell) = rec.placement.cell {
                        let cell = order.resolve(rref.file, raw_cell);
                        index.cells.entry(cell).or_default().deleted.push(id);
                    }
                }
                continue;
            }
            if rec.tag == tag::CELL {
                index.add_cell(order, id, rref.file, rec);
                continue;
            }
            let Some(raw_cell) = rec.placement.cell else {
                continue;
            };
            let cell = order.resolve(rref.file, raw_cell);
            if PLACED_TAGS.contains(&rec.tag) {
                let list = if matches!(rec.placement.group, Some(GroupKind::CellPersistent(_))) {
                    &mut persistent
                } else {
                    &mut temporary
                };
                list.entry(cell).or_default().push(id);
            } else if rec.tag == tag::LAND {
                index.cells.entry(cell).or_default().land = Some(id);
            } else if rec.tag == tag::NAVM {
                index.cells.entry(cell).or_default().navmeshes.push(id);
            }
        }
        for (cell, mut ids) in persistent {
            ids.sort_unstable();
            let entry = index.cells.entry(cell).or_default();
            entry.persistent = ids.len();
            entry.placed = ids;
        }
        for (cell, mut ids) in temporary {
            ids.sort_unstable();
            index.cells.entry(cell).or_default().placed.extend(ids);
        }
        for entry in index.cells.values_mut() {
            entry.navmeshes.sort_unstable();
            entry.deleted.sort_unstable();
            index.placed += entry.placed.len();
            index.deleted += entry.deleted.len();
        }
        span.set_payload(index.placed as u64);
        index
    }

    fn add_cell(&mut self, order: &LoadOrder, id: GlobalFormId, file: usize, rec: &Record) {
        let interior = rec
            .find(Tag::new(b"DATA"))
            .and_then(|d| d.first())
            .is_some_and(|b| b & 1 != 0);
        self.cells.entry(id).or_default().interior = interior;
        if let Some(edid) = rec.editor_id() {
            let replace = match self.by_editor_id.get(&edid) {
                None => true,
                Some(old) => {
                    let old_interior = self.cells.get(old).is_some_and(|e| e.interior);
                    (interior && !old_interior) || (interior == old_interior && id < *old)
                }
            };
            if replace {
                self.by_editor_id.insert(edid, id);
            }
        }
        let Some(raw_world) = rec.placement.world else {
            return;
        };
        let world = order.resolve(file, raw_world);
        if matches!(rec.placement.group, Some(GroupKind::WorldChildren(_))) {
            self.world_cells.insert(world, id);
            return;
        }
        if let Some(d) = rec.find(Tag::new(b"XCLC")) {
            if d.len() >= 8 {
                let x = i32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                let y = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
                let grid = GridCell { world, x, y };
                self.grid.insert(grid, id);
                self.grid_of.insert(id, grid);
            }
        }
    }

    /// Every placed object whose winning record sits in `cell`, persistent
    /// ones first. Empty for a cell the index does not know.
    #[must_use]
    pub fn references(&self, cell: GlobalFormId) -> &[GlobalFormId] {
        self.cells.get(&cell).map_or(&[], |e| e.placed.as_slice())
    }

    #[must_use]
    pub fn persistent_references(&self, cell: GlobalFormId) -> &[GlobalFormId] {
        self.cells
            .get(&cell)
            .map_or(&[], |e| &e.placed[..e.persistent])
    }

    #[must_use]
    pub fn temporary_references(&self, cell: GlobalFormId) -> &[GlobalFormId] {
        self.cells
            .get(&cell)
            .map_or(&[], |e| &e.placed[e.persistent..])
    }

    /// Placed objects filed under `cell` whose winning record is deleted.
    /// They resolve through the load order and start disabled, they are
    /// never drawn, so they are not in `references`.
    #[must_use]
    pub fn deleted_references(&self, cell: GlobalFormId) -> &[GlobalFormId] {
        self.cells.get(&cell).map_or(&[], |e| e.deleted.as_slice())
    }

    #[must_use]
    pub fn land(&self, cell: GlobalFormId) -> Option<GlobalFormId> {
        self.cells.get(&cell).and_then(|e| e.land)
    }

    #[must_use]
    pub fn navmeshes(&self, cell: GlobalFormId) -> &[GlobalFormId] {
        self.cells
            .get(&cell)
            .map_or(&[], |e| e.navmeshes.as_slice())
    }

    /// Whether the cell's winning record carries the interior flag. False
    /// for a cell the index only knows through its children.
    #[must_use]
    pub fn is_interior(&self, cell: GlobalFormId) -> bool {
        self.cells.get(&cell).is_some_and(|e| e.interior)
    }

    /// The exterior cell at a grid position of a worldspace.
    #[must_use]
    pub fn exterior_cell(&self, world: GlobalFormId, x: i32, y: i32) -> Option<GlobalFormId> {
        self.grid.get(&GridCell { world, x, y }).copied()
    }

    /// Where an exterior cell sits. `None` for interiors and for a
    /// worldspace's persistent cell.
    #[must_use]
    pub fn grid_of(&self, cell: GlobalFormId) -> Option<GridCell> {
        self.grid_of.get(&cell).copied()
    }

    /// The cell that holds a worldspace's persistent references.
    #[must_use]
    pub fn world_persistent_cell(&self, world: GlobalFormId) -> Option<GlobalFormId> {
        self.world_cells.get(&world).copied()
    }

    /// The cell with this editor id. When an interior and an exterior cell
    /// share one, the interior wins, then the lowest id.
    #[must_use]
    pub fn cell_by_editor_id(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.by_editor_id.get(editor_id).copied()
    }

    /// Every cell the index knows with its placed object count, in no
    /// particular order.
    pub fn cells(&self) -> impl Iterator<Item = (GlobalFormId, usize)> + '_ {
        self.cells.iter().map(|(id, e)| (*id, e.placed.len()))
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn placed_count(&self) -> usize {
        self.placed
    }

    /// Placed objects across every cell whose winner is deleted.
    #[must_use]
    pub fn deleted_count(&self) -> usize {
        self.deleted
    }

    #[must_use]
    pub fn grid_count(&self) -> usize {
        self.grid.len()
    }

    #[must_use]
    pub fn world_count(&self) -> usize {
        self.world_cells.len()
    }
}
