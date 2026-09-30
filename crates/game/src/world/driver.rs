// SPDX-License-Identifier: GPL-3.0-or-later

//! What the package driver (`crate::ai`) reads and calls on the world: the
//! loaded actors and quests, the player, editor positions, cell flags, and
//! the `EvaluatePackage` and tick entry points.

use soe_esm::records::refr::Refr;
use soe_esm::GlobalFormId;

use super::{le_f32, Alloc, World, T};

impl World {
    /// Every actor whose run-time state has been built.
    pub fn loaded_actors(&self) -> Vec<GlobalFormId> {
        let mut out: Vec<GlobalFormId> = self.actors.keys().copied().collect();
        out.sort();
        out
    }

    /// Every quest whose run-time state has been built.
    pub fn loaded_quests(&self) -> Vec<GlobalFormId> {
        let mut out: Vec<GlobalFormId> = self.quests.keys().copied().collect();
        out.sort();
        out
    }

    pub fn is_player(&self, id: GlobalFormId) -> bool {
        self.player == Some(id)
    }

    /// The position the reference was placed at in its record.
    pub fn editor_position(&self, id: GlobalFormId) -> Option<[f32; 3]> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        match rec.tag.as_bytes() {
            b"REFR" => Refr::from_record(rec).ok()?.transform.map(|t| t.position),
            b"ACHR" => {
                let d = rec.find(T(b"DATA")).filter(|d| d.len() >= 12)?;
                Some([le_f32(d), le_f32(&d[4..]), le_f32(&d[8..])])
            }
            _ => None,
        }
    }

    /// The interior flag of a `CELL` record's `DATA`.
    pub fn cell_is_interior(&self, cell: GlobalFormId) -> bool {
        self.record(cell)
            .filter(|r| r.tag == T(b"CELL"))
            .and_then(|r| r.find(T(b"DATA")))
            .and_then(|d| d.first())
            .is_some_and(|f| f & 1 != 0)
    }

    /// Whether the exterior cell belongs to a worldspace with max-height data.
    pub fn cell_has_worldspace_max_height_data(&self, cell: GlobalFormId) -> bool {
        let Some(cell_ref) = self.order.winner_ref(cell) else {
            return false;
        };
        let cell_record = self.order.record(cell_ref);
        if cell_record.tag != T(b"CELL") {
            return false;
        }
        let Some(raw_world) = cell_record.placement.world else {
            return false;
        };
        let worldspace = self.order.resolve(cell_ref.file, raw_world);
        self.record(worldspace)
            .filter(|record| record.tag == T(b"WRLD"))
            .is_some_and(|record| record.find(T(b"MHDT")).is_some())
    }

    /// Build the run-time state of an actor so the driver ticks it.
    pub fn drive_actor(&mut self, id: GlobalFormId) -> bool {
        self.actor(id).is_some()
    }

    /// `Actor.EvaluatePackage`: re-evaluate the stack now.
    pub fn evaluate_package(
        &mut self,
        actor: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Option<GlobalFormId> {
        crate::ai::evaluate(self, actor, alloc)
    }

    /// `Actor.GetCurrentPackage`.
    pub fn current_package(&self, actor: GlobalFormId) -> Option<GlobalFormId> {
        self.actors.get(&actor).and_then(|a| a.ai.package)
    }

    /// Advance every driven actor by `real_seconds`.
    pub fn ai_tick(&mut self, real_seconds: f64, alloc: Alloc<'_>) {
        crate::ai::tick(self, real_seconds, alloc);
    }
}
