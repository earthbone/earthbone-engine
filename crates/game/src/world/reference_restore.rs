// SPDX-License-Identifier: GPL-3.0-or-later

//! World identities and placement from ESS reference prefixes and player
//! location. xEdit/UESP define the fields. Remaining payloads stay explicit
//! restoration requirements, rather than being treated as a complete load.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_save::{
    reference::{Initial, ReferenceHeader},
    Save,
};

use super::{RefState, World, DYNAMIC_FILE, PLAYER_OBJECT, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

/// Reference state that this placement-only pass has not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRemainder {
    pub change_flags: u32,
    pub suffix_bytes: usize,
    pub havok_bytes: usize,
    pub actor_header: Option<[u32; 2]>,
    pub form_flags: Option<(u32, u16)>,
    pub creation_flags: Option<u8>,
}

/// A parent identity or exterior grid cell absent from the current load order.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MissingPlacementCell {
    pub parent: GlobalFormId,
    pub coordinates: Option<[i32; 2]>,
}

/// Applied identities/placements and prerequisites for restoring the remainder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceRestoreReport {
    pub references: usize,
    pub created: usize,
    pub player: GlobalFormId,
    pub remaining: BTreeMap<GlobalFormId, ReferenceRemainder>,
    pub missing_bases: BTreeSet<GlobalFormId>,
    pub missing_cells: BTreeSet<MissingPlacementCell>,
    pub unplaced: BTreeSet<GlobalFormId>,
}

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::WorldReference {
        id,
        reason: reason.into(),
    }
}

fn cell(
    world: &World,
    report: &mut ReferenceRestoreReport,
    parent: Option<GlobalFormId>,
    position: [f32; 3],
) -> Result<Option<GlobalFormId>, RestoreError> {
    let Some(parent) = parent else {
        return Ok(None);
    };
    match world.order.winner(parent) {
        Some(record) if !record.is_deleted() && record.tag == T(b"CELL") => Ok(Some(parent)),
        Some(record) if !record.is_deleted() && record.tag == T(b"WRLD") => {
            let grid = [
                f64::from(position[0]) / 4096.0,
                f64::from(position[1]) / 4096.0,
            ];
            if grid.iter().any(|value| {
                value.floor() < f64::from(i32::MIN) || value.floor() > f64::from(i32::MAX)
            }) {
                return Err(invalid(parent, "position exceeds the cell grid"));
            }
            let coordinates = [grid[0].floor() as i32, grid[1].floor() as i32];
            let cell = world.order.build_cell_index().exterior_cell(
                parent,
                coordinates[0],
                coordinates[1],
            );
            if cell.is_none() {
                report.missing_cells.insert(MissingPlacementCell {
                    parent,
                    coordinates: Some(coordinates),
                });
            }
            Ok(cell)
        }
        Some(record) if !record.is_deleted() => {
            Err(invalid(parent, "placement parent is neither CELL nor WRLD"))
        }
        _ => {
            report.missing_cells.insert(MissingPlacementCell {
                parent,
                coordinates: None,
            });
            Ok(None)
        }
    }
}

fn apply_header(
    world: &World,
    mapping: &SavedFormMap<'_>,
    report: &mut ReferenceRestoreReport,
    id: GlobalFormId,
    state: &mut RefState,
    header: &ReferenceHeader,
) -> Result<(), RestoreError> {
    if let Some(placement) = header.initial.placement() {
        if placement
            .position
            .iter()
            .chain(&placement.rotation)
            .any(|v| !v.is_finite())
        {
            return Err(invalid(id, "non-finite reference transform"));
        }
        state.position = placement.position;
        state.rotation = placement.rotation;
        state.cell = cell(
            world,
            report,
            mapping.reference(placement.cell_or_world)?,
            placement.position,
        )?;
    }
    let created_base = match header.initial {
        Initial::Created { base, .. } => Some(base),
        _ => None,
    };
    if let Some(base) = header.base.or(created_base) {
        state.base = mapping.reference(base)?;
    }
    if let Some(scale) = header.scale {
        if !scale.factor().is_finite() {
            return Err(invalid(id, "non-finite reference scale"));
        }
        state.scale = scale.factor();
    }
    if let Some(base) = state.base {
        if world
            .order
            .winner(base)
            .is_none_or(|record| record.is_deleted())
        {
            report.missing_bases.insert(base);
        }
    }
    if state.cell.is_none() {
        report.unplaced.insert(id);
    }
    report.remaining.insert(
        id,
        ReferenceRemainder {
            change_flags: header.change_flags,
            suffix_bytes: header.remaining_bytes,
            havok_bytes: header.havok.as_ref().map_or(0, |value| value.data.len()),
            actor_header: header.actor,
            form_flags: header.form_flags,
            creation_flags: match header.initial {
                Initial::Created { flags, .. } => Some(flags),
                _ => None,
            },
        },
    );
    Ok(())
}

impl World {
    /// Build saved REFR/ACHR identities and placements in a fresh reference table.
    /// This is one pass of save reconstruction. The report lists unapplied
    /// payloads. No inventory, actor, script, cell or activation event runs.
    ///
    /// # Errors
    /// Existing references/actors, invalid mapping, malformed placement headers,
    /// duplicate identities, bad transforms, invalid parents or player data.
    pub fn restore_vanilla_reference_placements(
        &mut self,
        save: &Save,
    ) -> Result<ReferenceRestoreReport, RestoreError> {
        let player = GlobalFormId {
            file: 0,
            object: PLAYER_OBJECT,
        };
        if !self.refs.is_empty() || !self.actors.is_empty() || !self.actor_templates.is_empty() {
            return Err(invalid(
                player,
                "reference reconstruction requires a fresh reference table",
            ));
        }
        let mapping = SavedFormMap::new(save, &self.order)?;
        let location = save
            .player_location()?
            .ok_or_else(|| invalid(player, "missing player location"))?;
        if location.next_object_id >> 24 != 0xFF || location.next_object_id.trailing_zeros() >= 24 {
            return Err(invalid(player, "invalid next created-object identity"));
        }
        if location.position.iter().any(|value| !value.is_finite()) {
            return Err(invalid(player, "non-finite player position"));
        }
        let mut next_dynamic = (location.next_object_id & 0xFFFFFF) - 1;
        // CONTEXT: Created NPCs and objects share an allocation namespace.
        // Reserve every saved identity before a future PlaceAtMe allocates one.
        for form in &save.change_forms {
            if let Some(id) = mapping.reference(form.form)? {
                if id.file == DYNAMIC_FILE {
                    next_dynamic = next_dynamic.max(id.object);
                }
            }
        }
        for &raw in &save.form_id_array {
            if raw >> 24 == 0xFF {
                next_dynamic = next_dynamic.max(raw & 0xFFFFFF);
            }
        }
        if next_dynamic == 0xFFFFFF {
            return Err(invalid(player, "created-object identity space exhausted"));
        }
        let mut report = ReferenceRestoreReport {
            player,
            references: 0,
            created: 0,
            remaining: BTreeMap::new(),
            missing_bases: BTreeSet::new(),
            missing_cells: BTreeSet::new(),
            unplaced: BTreeSet::new(),
        };
        let mut references = BTreeMap::new();
        for form in &save.change_forms {
            let Some(header) = form.reference_header(save.form_version)? else {
                continue;
            };
            let id = mapping
                .reference(form.form)?
                .ok_or(RestoreError::Reference(form.form.raw()))?;
            if references.contains_key(&id) {
                return Err(invalid(id, "duplicate reference change form"));
            }
            let mut state = if id.file == DYNAMIC_FILE {
                if !matches!(header.initial, Initial::Created { .. }) {
                    return Err(invalid(id, "created reference has no creation header"));
                }
                report.created += 1;
                RefState::new(None)
            } else if id == player {
                let base = self
                    .form("Player")
                    .filter(|id| self.tag_of(*id) == Some(T(b"NPC_")));
                RefState::new(base)
            } else {
                let record = self
                    .order
                    .winner(id)
                    .ok_or_else(|| invalid(id, "missing reference record"))?;
                if record.tag
                    != if form.type_index() == 1 {
                        T(b"ACHR")
                    } else {
                        T(b"REFR")
                    }
                {
                    return Err(invalid(id, "reference record type differs from save"));
                }
                self.load_ref_record(id)
                    .ok_or_else(|| invalid(id, "malformed reference record"))?
            };
            state.is_actor = form.type_index() == 1;
            apply_header(self, &mapping, &mut report, id, &mut state, &header)?;
            references.insert(id, state);
        }
        let state = references
            .get_mut(&player)
            .ok_or_else(|| invalid(player, "missing player ACHR change form"))?;
        if !state.is_actor {
            return Err(invalid(player, "player change form is not ACHR"));
        }
        if state.position.map(f32::to_bits) != location.position.map(f32::to_bits) {
            return Err(invalid(
                player,
                "player location disagrees with ACHR placement",
            ));
        }
        state.cell = cell(
            self,
            &mut report,
            mapping.reference(location.cell_or_world)?,
            location.position,
        )?;
        let player_cell = state.cell;
        if player_cell.is_some() {
            report.unplaced.remove(&player);
        }
        report.references = references.len();
        self.refs.extend(references);
        self.player = Some(player);
        self.loaded_cell = player_cell;
        self.next_dynamic = next_dynamic;
        Ok(report)
    }
}
