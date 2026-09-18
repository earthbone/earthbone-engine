// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell transitions: what happens when the player walks through a load door.
//!
//! Vanilla splits a cell's children into a persistent group and a temporary
//! one, and only the temporary half comes and goes with the cell. This keeps
//! that split, and adds a rule vanilla does not need because vanilla writes
//! change forms into a save: a reference is dropped only when rebuilding it
//! from its record reproduces exactly the state it holds now. Anything a
//! script has touched stays resident, so unloading can never lose a change.
//!
//! The player, actors and dynamically placed references are never dropped.
//! An actor's state lives in a second map, a dynamic reference has no record
//! to rebuild from, and the player is the thing doing the walking.

use soe_esm::GlobalFormId;

use super::{World, DYNAMIC_FILE};

/// What one cell change moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellTransition {
    pub from: Option<GlobalFormId>,
    pub to: Option<GlobalFormId>,
    /// Temporary references dropped, each proved rebuildable first.
    pub unloaded: usize,
    /// References the old cell owned that were kept, because dropping them
    /// would have lost a change.
    pub kept: usize,
    /// References built for the destination.
    pub loaded: usize,
}

impl World {
    /// The cell whose temporary references are resident, if a transition has
    /// put one there. `None` before the first one, when references are simply
    /// built on first use.
    #[must_use]
    pub fn loaded_cell(&self) -> Option<GlobalFormId> {
        self.loaded_cell
    }

    /// Every placed object the index files under `cell`, built now rather
    /// than on first use. Returns how many were built here, so a reference
    /// already resident is not counted twice.
    pub fn load_cell(&mut self, cell: GlobalFormId) -> usize {
        let ids = self.cell_reference_ids(cell);
        let mut built = 0;
        for id in ids {
            if self.refs.contains_key(&id) {
                continue;
            }
            if self.reference(id).is_some() {
                built += 1;
            }
        }
        self.loaded_cell = Some(cell);
        built
    }

    /// Drop what `cell` owns and nothing else. Returns the count dropped and
    /// the count kept because dropping would have lost something.
    pub fn unload_cell(&mut self, cell: GlobalFormId) -> (usize, usize) {
        let player = self.player();
        let ids = self.cell_temporary_ids(cell);
        let (mut dropped, mut kept) = (0, 0);
        for id in ids {
            if !self.refs.contains_key(&id) {
                continue;
            }
            if id == player || id.file == DYNAMIC_FILE {
                kept += 1;
                continue;
            }
            // An actor's run-time state sits in a second map keyed the same
            // way, so dropping half of it would leave the world inconsistent.
            if self.refs.get(&id).is_some_and(|r| r.is_actor) {
                kept += 1;
                continue;
            }
            // The rule that makes this lossless: rebuild the reference from
            // its record and drop it only when the rebuild matches. A script
            // that moved, disabled, unlocked or emptied it fails this and the
            // reference stays.
            let fresh = self.load_ref(id);
            let pristine = match (fresh, self.refs.get(&id)) {
                (Some(a), Some(b)) => &a == b,
                _ => false,
            };
            if pristine {
                self.refs.remove(&id);
                dropped += 1;
            } else {
                kept += 1;
            }
        }
        (dropped, kept)
    }

    /// Unload the resident cell and load `to`. The clock is untouched, so
    /// time keeps running across the move the way it does in vanilla.
    pub fn change_cell(&mut self, to: Option<GlobalFormId>) -> CellTransition {
        let from = self.loaded_cell;
        let mut out = CellTransition {
            from,
            to,
            ..CellTransition::default()
        };
        if from == to {
            return out;
        }
        if let Some(old) = from {
            let (dropped, kept) = self.unload_cell(old);
            out.unloaded = dropped;
            out.kept = kept;
        }
        if let Some(new) = to {
            out.loaded = self.load_cell(new);
        } else {
            self.loaded_cell = None;
        }
        out
    }

    /// How many references are resident, across every cell.
    #[must_use]
    pub fn loaded_reference_count(&self) -> usize {
        self.refs.len()
    }

    /// Whether a reference is resident, without building it the way
    /// [`World::reference`] would.
    #[must_use]
    pub fn is_reference_loaded(&self, id: GlobalFormId) -> bool {
        self.refs.contains_key(&id)
    }

    /// Whether the cell index files a reference in the cell's persistent
    /// group rather than its temporary one. A persistent child outlives the
    /// cell, so an unload leaves it alone.
    pub fn is_persistent_child(&mut self, cell: GlobalFormId, id: GlobalFormId) -> bool {
        if self.order.cell_index().is_none() {
            self.order.build_cell_index();
        }
        match self.order.cell_index() {
            Some(index) => index.persistent_references(cell).contains(&id),
            None => false,
        }
    }

    /// A cell by its editor id, interior first. The index answers this, and
    /// builds itself on the first ask.
    pub fn cell_by_editor_id(&mut self, editor_id: &str) -> Option<GlobalFormId> {
        if self.order.cell_index().is_none() {
            self.order.build_cell_index();
        }
        self.order.cell_index()?.cell_by_editor_id(editor_id)
    }

    /// Every door placed in a cell, with the far door when it is a load door
    /// and `None` when it simply swings.
    pub fn cell_doors(&mut self, cell: GlobalFormId) -> Vec<(GlobalFormId, Option<GlobalFormId>)> {
        let ids = self.cell_reference_ids(cell);
        let mut out = Vec::new();
        for id in ids {
            let Some(base) = self.base_of(id) else {
                continue;
            };
            if self.tag_of(base) != Some(super::T(b"DOOR")) {
                continue;
            }
            let far = self.reference(id).and_then(|r| r.teleport).map(|(d, _)| d);
            out.push((id, far));
        }
        out
    }

    /// Every reference the index files under a cell, persistent and
    /// temporary. Collected into a vector because the index borrows the load
    /// order and building a reference needs the world.
    fn cell_reference_ids(&mut self, cell: GlobalFormId) -> Vec<GlobalFormId> {
        if self.order.cell_index().is_none() {
            self.order.build_cell_index();
        }
        match self.order.cell_index() {
            Some(index) => index.references(cell).to_vec(),
            None => Vec::new(),
        }
    }

    /// The temporary half alone, which is what a cell change unloads.
    fn cell_temporary_ids(&mut self, cell: GlobalFormId) -> Vec<GlobalFormId> {
        if self.order.cell_index().is_none() {
            self.order.build_cell_index();
        }
        match self.order.cell_index() {
            Some(index) => index.temporary_references(cell).to_vec(),
            None => Vec::new(),
        }
    }
}
