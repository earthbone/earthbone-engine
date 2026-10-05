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
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Reader, Writer};
use soe_papyrus::{ObjectId, VmError};

use super::save::{read_form_id, write_form_id, FormReader};
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
    pub(crate) fn record_session_residency(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        for set in [
            &self.streamed_cells,
            &self.loaded_3d_references,
            &self.attached_cells,
        ] {
            let mut ids: Vec<_> = set.iter().copied().collect();
            ids.sort_unstable();
            writer.len(ids.len());
            for id in ids {
                writer.u16(id.file);
                writer.u32(id.object);
            }
        }
        writer.bool(self.pending_unload.is_some());
        if let Some(id) = self.pending_unload {
            writer.u16(id.file);
            writer.u32(id.object);
        }
        writer.out
    }

    pub(crate) fn restore_session_residency(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let mut reader = Reader::new(bytes);
        let mut sets = Vec::new();
        for _ in 0..3 {
            let mut ids = std::collections::HashSet::new();
            for _ in 0..reader.len()? {
                let id = GlobalFormId {
                    file: reader.u16()?,
                    object: reader.u32()?,
                };
                if !ids.insert(id) {
                    return Err(VmError::State("duplicate session residency id".into()));
                }
            }
            sets.push(ids);
        }
        let pending_unload = if reader.bool()? {
            Some(GlobalFormId {
                file: reader.u16()?,
                object: reader.u32()?,
            })
        } else {
            None
        };
        reader.expect_end()?;
        let mut sets = sets.into_iter();
        self.streamed_cells = sets.next().unwrap_or_default();
        self.loaded_3d_references = sets.next().unwrap_or_default();
        self.attached_cells = sets.next().unwrap_or_default();
        self.pending_unload = pending_unload;
        Ok(())
    }

    /// Whether the cell currently has streamed host resources.
    #[must_use]
    pub fn cell_is_attached(&self, cell: GlobalFormId) -> bool {
        self.attached_cells.contains(&cell)
    }

    pub(crate) fn attached_cell_ids(&self) -> Vec<GlobalFormId> {
        let mut cells: Vec<_> = self.attached_cells.iter().copied().collect();
        if let Some(cell) = self.loaded_cell {
            cells.push(cell);
        }
        cells.sort_unstable();
        cells.dedup();
        cells
    }

    pub(crate) fn write_reference_events(&self, out: &mut Writer, _pool: &mut ArrayPool) {
        let events: Vec<_> = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(index, event)| match event {
                Pending::ReferenceEvent { reference, name } => Some((index, reference, name)),
                _ => None,
            })
            .collect();
        out.len(events.len());
        for (index, reference, name) in events {
            out.len(index);
            write_form_id(out, *reference);
            out.str(name);
        }
    }

    pub(crate) fn read_reference_events(
        world: &mut World,
        input: &mut FormReader<'_, '_>,
        _table: &ArrayTable,
    ) -> Result<(), VmError> {
        let mut previous = None;
        for _ in 0..input.len()? {
            let index = input.len()?;
            let reference = read_form_id(input)?;
            let name = input.str()?;
            if previous.is_some_and(|prior| index <= prior) || index > world.pending.len() {
                return Err(VmError::State(
                    "SCR-01 invalid reference event position".into(),
                ));
            }
            previous = Some(index);
            world
                .pending
                .insert(index, Pending::ReferenceEvent { reference, name });
        }
        Ok(())
    }

    /// Mark a cell's temporary references to be restored from their records
    /// the next time the cell is loaded.
    ///
    /// # Errors
    /// Returns an error when `cell` is not a live CELL form.
    pub fn reset_cell(&mut self, cell: GlobalFormId) -> soe_esm::Result<()> {
        if !self
            .order
            .winner(cell)
            .is_some_and(|record| record.tag == super::T(b"CELL") && !record.is_deleted())
        {
            return Err(soe_esm::Error::Invalid("invalid cell reset target".into()));
        }
        self.pending_cell_resets.insert(cell);
        Ok(())
    }

    fn reset_cell_on_load(&mut self, cell: GlobalFormId) {
        if !self.pending_cell_resets.remove(&cell) {
            return;
        }
        let player = self.player;
        for id in self.cell_temporary_ids(cell) {
            if Some(id) == player || id.file == DYNAMIC_FILE {
                continue;
            }
            let persistent = self.order.winner_ref(id).is_some_and(|record_ref| {
                self.order.record(record_ref).flags & soe_esm::plugin::flags::PERSISTENT != 0
            });
            if persistent {
                continue;
            }
            self.refs.remove(&id);
            self.actors.remove(&id);
            self.actor_templates.remove(&id);
            let _ = self.reference(id);
        }
    }

    pub(crate) fn write_pending_cell_resets(&self, out: &mut Writer, _pool: &mut ArrayPool) {
        out.len(self.pending_cell_resets.len());
        for cell in &self.pending_cell_resets {
            write_form_id(out, *cell);
        }
    }

    pub(crate) fn read_pending_cell_resets(
        world: &mut World,
        input: &mut FormReader<'_, '_>,
        _table: &ArrayTable,
    ) -> Result<(), VmError> {
        let mut cells = std::collections::BTreeSet::new();
        for _ in 0..input.len()? {
            let cell = read_form_id(input)?;
            if !world
                .order
                .winner(cell)
                .is_some_and(|record| record.tag == super::T(b"CELL") && !record.is_deleted())
                || !cells.insert(cell)
            {
                return Err(VmError::State(
                    "invalid or duplicate pending cell reset".into(),
                ));
            }
        }
        world.pending_cell_resets = cells;
        Ok(())
    }

    /// Apply a host residency delta without detaching retained neighboring cells.
    /// Entered cells have just acquired host resources, even if their records
    /// were prefetched. The caller supplies each resource transition once.
    /// Cell event audiences follow the CK OnCellAttach and OnCellDetach pages.
    ///
    /// # Errors
    /// Rejects invalid cells, overlapping or duplicate transitions, a departed
    /// focus, and an unsettled load-door transition before changing state.
    pub fn update_streaming_cells(
        &mut self,
        focus: GlobalFormId,
        entered: &[GlobalFormId],
        departed: &[GlobalFormId],
    ) -> soe_esm::Result<CellTransition> {
        use std::collections::BTreeSet;

        let entering: BTreeSet<_> = entered.iter().copied().collect();
        let leaving: BTreeSet<_> = departed.iter().copied().collect();
        let valid = |id| {
            self.order
                .winner(id)
                .is_some_and(|r| r.tag == super::T(b"CELL") && !r.is_deleted())
        };
        if self.pending_unload.is_some()
            || entering.len() != entered.len()
            || leaving.len() != departed.len()
            || !entering.is_disjoint(&leaving)
            || leaving.contains(&focus)
            || (!self.cell_is_resident(focus) && !entering.contains(&focus))
            || !valid(focus)
            || entering.iter().chain(&leaving).any(|id| !valid(*id))
        {
            return Err(soe_esm::Error::Invalid(
                "invalid host cell residency delta".into(),
            ));
        }
        if self.loaded_cell != Some(focus) {
            self.enter_cell_zone(focus)?;
        }
        let mut out = CellTransition {
            from: self.loaded_cell,
            to: Some(focus),
            ..CellTransition::default()
        };
        for cell in leaving {
            self.attached_cells.remove(&cell);
            if self.cell_is_resident(cell) {
                let targets = self.scripted_in_cell(cell);
                self.queue_ref_event(&targets, "OnCellDetach", false);
                self.queue_ref_event(&targets, "OnUnload", true);
                let (dropped, kept) = self.unload_cell(cell);
                out.unloaded += dropped;
                out.kept += kept;
            }
        }
        for cell in entering {
            let cold = !self.cell_is_resident(cell);
            out.loaded += self.prefetch_cell(cell);
            self.mark_cell_3d_loaded(cell);
            self.reset_cell_on_load(cell);
            self.attached_cells.insert(cell);
            let targets = self.scripted_in_cell(cell);
            self.queue_ref_event(&targets, "OnCellAttach", false);
            self.queue_ref_event(&targets, "OnLoad", true);
            if cold {
                self.queue_ref_event(&targets, "OnCellLoad", false);
            }
        }
        self.loaded_cell = Some(focus);
        let player = self.player();
        if let Some(reference) = self.reference(player) {
            reference.cell = Some(focus);
        }
        Ok(out)
    }

    /// Attach neighboring cells whose host resources have just become resident.
    /// The player focus stays unchanged. Each resource arrival must be supplied once.
    ///
    /// # Errors
    /// Rejects duplicate, invalid or focused cells before changing state.
    pub fn attach_streaming_cells(&mut self, cells: &[GlobalFormId]) -> soe_esm::Result<usize> {
        let unique: std::collections::BTreeSet<_> = cells.iter().copied().collect();
        if unique.len() != cells.len()
            || unique.iter().any(|id| {
                Some(*id) == self.loaded_cell
                    || !self
                        .order
                        .winner(*id)
                        .is_some_and(|r| r.tag == super::T(b"CELL") && !r.is_deleted())
            })
        {
            return Err(soe_esm::Error::Invalid(
                "invalid neighboring cell attachment".into(),
            ));
        }
        // A short door move can bring the departed focus back as a neighbor.
        // Complete its deferred unload before attaching its new host resources.
        if self.pending_unload.is_some_and(|id| unique.contains(&id)) {
            self.settle_unload();
        }
        let mut loaded = 0;
        for cell in unique {
            if self.attached_cells.contains(&cell) {
                continue;
            }
            let cold = !self.cell_is_resident(cell);
            loaded += self.prefetch_cell(cell);
            self.mark_cell_3d_loaded(cell);
            self.reset_cell_on_load(cell);
            self.attached_cells.insert(cell);
            let targets = self.scripted_in_cell(cell);
            self.queue_ref_event(&targets, "OnCellAttach", false);
            self.queue_ref_event(&targets, "OnLoad", true);
            if cold {
                self.queue_ref_event(&targets, "OnCellLoad", false);
            }
        }
        Ok(loaded)
    }

    /// Detach neighboring cells when their host resources leave residency.
    /// The player focus and deferred load-door unload stay unchanged.
    ///
    /// # Errors
    /// Rejects duplicate, invalid, focused or pending cells before changing state.
    pub fn detach_streaming_cells(
        &mut self,
        cells: &[GlobalFormId],
    ) -> soe_esm::Result<(usize, usize)> {
        let unique: std::collections::BTreeSet<_> = cells.iter().copied().collect();
        if unique.len() != cells.len()
            || unique.iter().any(|id| {
                Some(*id) == self.loaded_cell
                    || Some(*id) == self.pending_unload
                    || !self
                        .order
                        .winner(*id)
                        .is_some_and(|r| r.tag == super::T(b"CELL") && !r.is_deleted())
            })
        {
            return Err(soe_esm::Error::Invalid(
                "invalid neighboring cell detachment".into(),
            ));
        }
        let (mut dropped, mut kept) = (0, 0);
        for cell in unique {
            self.attached_cells.remove(&cell);
            if self.cell_is_resident(cell) {
                let targets = self.scripted_in_cell(cell);
                self.queue_ref_event(&targets, "OnCellDetach", false);
                self.queue_ref_event(&targets, "OnUnload", true);
                let counts = self.unload_cell(cell);
                dropped += counts.0;
                kept += counts.1;
            }
        }
        Ok((dropped, kept))
    }

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
        if self.enter_cell_zone(cell).is_err() {
            return 0;
        }
        let built = self.prefetch_cell(cell);
        crate::ai::nav_graph(self);
        self.mark_cell_3d_loaded(cell);
        self.reset_cell_on_load(cell);
        self.attached_cells.insert(cell);
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
        let _ = self.light_references_in_cell(cell);
        built
    }

    fn mark_cell_3d_loaded(&mut self, cell: GlobalFormId) {
        for id in self.cell_reference_ids(cell) {
            if self
                .refs
                .get(&id)
                .is_some_and(|reference| reference.enabled)
                && self.loaded_3d_references.insert(id)
            {
                self.commonlib_object_loaded(id, true);
            }
        }
        self.commonlib_cell_fully_loaded(cell);
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

    /// Whether a reference currently has 3D through its streamed cell.
    ///
    /// The host reports cell resource arrivals and departures through the
    /// streaming-cell APIs. This query uses that derived residency set, not
    /// whether a reference was merely built by a form lookup. Visibility and
    /// room-bound culling do not change the result while the cell stays
    /// resident. Disabled references have no loaded 3D.
    #[must_use]
    pub fn is_reference_3d_loaded(&self, id: GlobalFormId) -> bool {
        self.refs
            .get(&id)
            .is_some_and(|reference| reference.enabled && self.loaded_3d_references.contains(&id))
    }

    /// Drop what `cell` owns and nothing else. Returns the count dropped and
    /// the count kept because dropping would have lost something.
    pub fn unload_cell(&mut self, cell: GlobalFormId) -> (usize, usize) {
        let player = self.player();
        self.attached_cells.remove(&cell);
        // The 3D goes here, so the next entry owes `OnCellLoad` again.
        self.streamed_cells.remove(&cell);
        for id in self.cell_reference_ids(cell) {
            if self.loaded_3d_references.remove(&id) {
                self.commonlib_object_loaded(id, false);
            }
        }
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
        let mut out = self.attach_cell(to, cold);
        let (dropped, kept) = self.settle_unload();
        out.unloaded = dropped;
        out.kept = kept;
        out
    }

    /// Fire the whole event sequence and attach `to`, leaving the cell being
    /// left resident for [`World::settle_unload`] to drop.
    ///
    /// This is the half a player waits for, and it is deliberately all of the
    /// observable work and none of the expensive work. Dropping the old cell
    /// means rebuilding each of its references from its record to prove the
    /// reference is unchanged, which on a real interior costs more than
    /// building the destination did. A script cannot see any of that, so it
    /// does not belong on the frame the player crosses the threshold.
    ///
    /// The detach events still fire here, in order, before anything is
    /// dropped. Only the dropping moves.
    pub fn attach_cell(&mut self, to: Option<GlobalFormId>, cold: bool) -> CellTransition {
        if to.is_some_and(|cell| self.enter_cell_zone(cell).is_err()) {
            return CellTransition {
                from: self.loaded_cell,
                to: self.loaded_cell,
                ..CellTransition::default()
            };
        }
        // At most one cell is ever waiting. A caller that never settles pays
        // for it on its next transition instead of leaking a cell per door.
        self.settle_unload();
        let from = self.loaded_cell;
        let out = CellTransition {
            from,
            to,
            ..CellTransition::default()
        };
        if from == to {
            return out;
        }
        let mut out = out;
        if let Some(old) = from {
            // The cell detaches before its 3D goes, mirroring attach before
            // load on the way in. Both events are queued before anything is
            // dropped, so a script still sees the state it is being told
            // about.
            let leaving = self.scripted_in_cell(old);
            self.queue_ref_event(&leaving, "OnCellDetach", false);
            self.queue_ref_event(&leaving, "OnUnload", true);
            self.pending_unload = Some(old);
            self.attached_cells.remove(&old);
        }
        if let Some(new) = to {
            out.loaded = self.load_cell(new);
            self.mark_cell_3d_loaded(new);
            self.attached_cells.insert(new);
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

    /// Drop whatever a deferred attach left behind. Returns the counts
    /// dropped and kept, both zero when nothing is waiting.
    ///
    /// Safe to call on any frame and safe to call twice. Call it before
    /// saving, so a save does not carry a cell the player has left.
    pub fn settle_unload(&mut self) -> (usize, usize) {
        match self.pending_unload.take() {
            Some(old) => self.unload_cell(old),
            None => (0, 0),
        }
    }

    /// The cell waiting to be dropped, if any.
    #[must_use]
    pub fn pending_unload(&self) -> Option<GlobalFormId> {
        self.pending_unload
    }

    /// Include unbound VMAD carriers so lazy binding cannot erase cell events.
    fn scripted_in_cell(&mut self, cell: GlobalFormId) -> Vec<(GlobalFormId, Option<ObjectId>)> {
        let ids = self.cell_reference_ids(cell);
        ids.into_iter()
            .filter_map(|id| {
                let object = self.objects.get(&Handle::Form(id)).copied();
                let has_scripts = self
                    .record(id)
                    .is_some_and(|record| record.find(super::T(b"VMAD")).is_some());
                (object.is_some() || has_scripts).then_some((id, object))
            })
            .collect()
    }

    /// Queue one event to each target. `enabled_only` carries the wiki's rule
    /// that the 3D events skip a disabled reference, which has no 3D to load
    /// or unload, while the cell events do not.
    fn queue_ref_event(
        &mut self,
        targets: &[(GlobalFormId, Option<ObjectId>)],
        name: &str,
        enabled_only: bool,
    ) {
        for (id, object) in targets {
            if enabled_only && !self.refs.get(id).is_some_and(|r| r.enabled) {
                continue;
            }
            self.pending.push(match object {
                Some(object) => Pending::Event {
                    object: *object,
                    name: name.into(),
                    args: Vec::new(),
                },
                None => Pending::ReferenceEvent {
                    reference: *id,
                    name: name.into(),
                },
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
        if let Some(reference) = self.refs.get(&id) {
            return reference.persistent;
        }
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
    pub(crate) fn cell_reference_ids(&mut self, cell: GlobalFormId) -> Vec<GlobalFormId> {
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
