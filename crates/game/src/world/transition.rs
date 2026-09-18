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
//!
//! # Streaming and attaching are two different things
//!
//! Building a cell's references is slow and unobservable. Attaching a cell is
//! instant and is the entire script contract. Keeping them apart is what
//! removes the loading screen without changing what a mod sees: a prefetch
//! does the slow half early and queues nothing, then the attach fires the
//! whole vanilla event sequence in one go, in vanilla order, with nothing
//! left to build.
//!
//! One flag has to be carried across that gap rather than recomputed.
//! `OnCellLoad` fires only when the destination was not already in memory,
//! so reading residency after a prefetch would always say "in memory" and
//! the event would stop firing. [`World::change_cell_with`] takes the answer
//! captured before the prefetch instead.
//!
//! Event order and audience come from the Creation Kit wiki pages for each
//! event, under `../engine-refs/ck-wiki/`. `OnCellAttach` is documented as
//! reliably before `OnCellLoad` and as firing for disabled references, while
//! `OnLoad` is documented as not firing for them. The order between
//! `OnCellDetach` and `OnUnload` is not stated anywhere the corpus carries,
//! so this takes the symmetric reading, cell event first in both directions,
//! and the tests pin the choice rather than claim it is vanilla.

use soe_esm::GlobalFormId;
use soe_papyrus::ObjectId;

use super::{Handle, Pending, World, DYNAMIC_FILE};

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
        let built = self.prefetch_cell(cell);
        self.loaded_cell = Some(cell);
        built
    }

    /// Build every reference a cell owns without attaching the cell.
    ///
    /// This is the half of a transition that costs time, and it is engine
    /// state only: `loaded_cell` does not move, nothing is queued for the VM,
    /// and no script can tell it happened. A later `change_cell` onto the same
    /// cell then finds everything resident and does the attach in the time it
    /// takes to walk a list, which is what lets a door open with no loading
    /// screen. Returns how many references this call built.
    pub fn prefetch_cell(&mut self, cell: GlobalFormId) -> usize {
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
        self.streamed_cells.insert(cell);
        built
    }

    /// Whether the cell's 3D is in memory, which is what decides
    /// `OnCellLoad`.
    ///
    /// The wiki says the event fires only on entering a cell not already in
    /// memory, and warns it is unpredictable for exactly that reason. Read it
    /// before a prefetch, never after, or prefetching would silently stop the
    /// event.
    ///
    /// This asks whether the cell was streamed, not whether any of its
    /// references happen to exist. Those differ: any lookup that resolves a
    /// form builds that one reference, and one reference is not a loaded
    /// cell. Answering from resident references made an unrelated
    /// `object_of_form` suppress `OnCellLoad`, which the tests in
    /// `no_load_screen.rs` caught.
    pub fn cell_is_resident(&self, cell: GlobalFormId) -> bool {
        self.streamed_cells.contains(&cell)
    }

    /// Drop what `cell` owns and nothing else. Returns the count dropped and
    /// the count kept because dropping would have lost something.
    pub fn unload_cell(&mut self, cell: GlobalFormId) -> (usize, usize) {
        let player = self.player();
        // The 3D goes here, so the next entry owes `OnCellLoad` again.
        self.streamed_cells.remove(&cell);
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
    ///
    /// Reads whether the destination is cold now, so a direct call behaves
    /// the way it always did. A move that prefetched first must use
    /// [`World::change_cell_with`] and pass the coldness it captured before
    /// the prefetch ran.
    pub fn change_cell(&mut self, to: Option<GlobalFormId>) -> CellTransition {
        let cold = to.is_some_and(|c| !self.cell_is_resident(c));
        self.change_cell_with(to, cold)
    }

    /// The move itself, with the `OnCellLoad` decision supplied rather than
    /// read.
    ///
    /// Splitting that one flag out is what makes prefetching invisible to a
    /// script. Everything else here is already independent of when the
    /// references were built, so a prefetched move and a cold move queue the
    /// same events in the same order, and the only difference is that this
    /// call has nothing left to build.
    pub fn change_cell_with(&mut self, to: Option<GlobalFormId>, cold: bool) -> CellTransition {
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
            // The cell detaches before its 3D goes, mirroring attach before
            // load on the way in. Both events are queued before anything is
            // dropped, so a script still sees the state it is being told
            // about.
            let leaving = self.scripted_in_cell(old);
            self.queue_ref_event(&leaving, "OnCellDetach", false);
            self.queue_ref_event(&leaving, "OnUnload", true);
            let (dropped, kept) = self.unload_cell(old);
            out.unloaded = dropped;
            out.kept = kept;
        }
        if let Some(new) = to {
            out.loaded = self.load_cell(new);
            let arriving = self.scripted_in_cell(new);
            // `OnCellAttach` before `OnCellLoad` is the one ordering the wiki
            // states outright, and attach reaches disabled references while
            // the two 3D events do not.
            self.queue_ref_event(&arriving, "OnCellAttach", false);
            self.queue_ref_event(&arriving, "OnLoad", true);
            if cold {
                self.queue_ref_event(&arriving, "OnCellLoad", false);
            }
        } else {
            self.loaded_cell = None;
        }
        out
    }

    /// The references a cell owns that carry a script, with the VM object
    /// each one already has.
    ///
    /// A reference with no script never got an object, and vanilla sends it
    /// no event either, so this lookup is also the thing that stops a cell
    /// change from allocating an object per reference. Whiterun's Bannered
    /// Mare has hundreds of references and a handful of scripts.
    fn scripted_in_cell(&mut self, cell: GlobalFormId) -> Vec<(GlobalFormId, ObjectId)> {
        let ids = self.cell_reference_ids(cell);
        ids.into_iter()
            .filter_map(|id| self.objects.get(&Handle::Form(id)).map(|o| (id, *o)))
            .collect()
    }

    /// Queue one event to each target. `enabled_only` carries the wiki's rule
    /// that the 3D events skip a disabled reference, which has no 3D to load
    /// or unload, while the cell events do not.
    fn queue_ref_event(
        &mut self,
        targets: &[(GlobalFormId, ObjectId)],
        name: &str,
        enabled_only: bool,
    ) {
        for (id, object) in targets {
            if enabled_only && !self.refs.get(id).is_some_and(|r| r.enabled) {
                continue;
            }
            self.pending.push(Pending::Event {
                object: *object,
                name: name.into(),
                args: Vec::new(),
            });
        }
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
