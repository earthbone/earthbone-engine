// SPDX-License-Identifier: GPL-3.0-or-later

//! World identities and placement from ESS reference prefixes and player
//! location. xEdit/UESP define the fields. Remaining payloads stay explicit
//! restoration requirements, rather than being treated as a complete load.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{GlobalFormId, Record};
use soe_save::{
    reference::{Initial, ReferenceHeader},
    Save,
};

use super::record_overrides::RecordOverride;
use super::{RefState, World, DYNAMIC_FILE, PLAYER_OBJECT, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

/// Reference data left outside placement restoration, with applied mask words
/// retained for audit.
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

fn created_base_records(
    world: &World,
    save: &Save,
    mapping: &SavedFormMap<'_>,
) -> Result<BTreeMap<GlobalFormId, RecordOverride>, RestoreError> {
    let Some(objects) = save.created_objects()? else {
        return Ok(BTreeMap::new());
    };
    let mut records = BTreeMap::new();
    for (entries, tag) in [
        (&objects.weapon_enchantments, T(b"ENCH")),
        (&objects.armor_enchantments, T(b"ENCH")),
        (&objects.potions, T(b"ALCH")),
        (&objects.poisons, T(b"ALCH")),
    ] {
        for object in entries {
            let Some(id) = mapping.reference(object.form)? else {
                return Err(invalid(
                    GlobalFormId {
                        file: DYNAMIC_FILE,
                        object: 0,
                    },
                    "created base has a null identity",
                ));
            };
            if id.file != DYNAMIC_FILE {
                continue;
            }
            if world.record(id).is_some() || records.contains_key(&id) {
                return Err(invalid(id, "created base identity is already defined"));
            }
            let mut record = Record::new(tag, 0xFF00_0000 | id.object);
            if tag == T(b"ENCH") {
                // ENIT is the shared enchantment header. Fields not present in
                // the ESS table remain zero and are not inferred.
                record = record.sub(T(b"ENIT"), vec![0; 36]);
            } else {
                // The ESS table does not retain ALCH flags or value fields.
                record = record.sub(T(b"ENIT"), vec![0; 20]);
            }
            let mut value = RecordOverride::new(id, record);
            let mut used = BTreeSet::new();
            for effect in &object.effects {
                let target = mapping
                    .reference(effect.effect)?
                    .ok_or_else(|| invalid(id, "created effect has a null identity"))?;
                let raw = value
                    .link(&mut used, Some(target))
                    .map_err(|reason| invalid(id, reason))?;
                value
                    .record
                    .subrecords
                    .push(soe_esm::plugin::Subrecord::new(
                        T(b"EFID"),
                        raw.to_le_bytes().to_vec(),
                    ));
                let mut data = Vec::with_capacity(12);
                data.extend_from_slice(&effect.magnitude.to_le_bytes());
                data.extend_from_slice(&effect.area.to_le_bytes());
                data.extend_from_slice(&effect.duration.to_le_bytes());
                value
                    .record
                    .subrecords
                    .push(soe_esm::plugin::Subrecord::new(T(b"EFIT"), data));
            }
            records.insert(id, value);
        }
    }
    Ok(records)
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
    /// payloads and retains applied mask words. No inventory, actor, script,
    /// cell or activation event runs.
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
        let created_records = created_base_records(self, save, &mapping)?;
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
        // Created Objects identities can exist without a reference change
        // form or a FormID array entry. Reserve them before placement.
        if let Some(objects) = save.created_objects()? {
            for object in objects
                .weapon_enchantments
                .iter()
                .chain(&objects.armor_enchantments)
                .chain(&objects.potions)
                .chain(&objects.poisons)
            {
                if let Some(id) = mapping.reference(object.form)? {
                    if id.file == DYNAMIC_FILE {
                        next_dynamic = next_dynamic.max(id.object);
                    }
                }
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
            if header.form_flags.is_some()
                && (save.header.version != 12 || save.form_version != 78 || form.version != 78)
            {
                return Err(invalid(
                    GlobalFormId { file: 0, object: PLAYER_OBJECT },
                    "reference mask semantics are only validated for save version 12 and form versions 78",
                ));
            }
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
                let expected = match form.type_index() {
                    1 => T(b"ACHR"),
                    42 => T(b"PCON"),
                    47 => T(b"PARW"),
                    _ => T(b"REFR"),
                };
                if record.tag != expected {
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
        report
            .missing_bases
            .retain(|base| !created_records.contains_key(base));
        report.references = references.len();
        self.created_base_records.extend(created_records);
        self.refs.extend(references);
        self.player = Some(player);
        self.loaded_cell = player_cell;
        self.next_dynamic = next_dynamic;
        Ok(report)
    }
}

pub(super) fn write_created_base_section(
    world: &World,
    writer: &mut soe_papyrus::vm::state::Writer,
) {
    writer.len(world.created_base_records.len());
    for (id, value) in &world.created_base_records {
        super::save::write_form_id(writer, *id);
        writer.out.extend_from_slice(value.record.tag.as_bytes());
        writer.u32(value.record.flags);
        writer.len(value.record.subrecords.len());
        for subrecord in &value.record.subrecords {
            writer.out.extend_from_slice(subrecord.tag.as_bytes());
            writer.bytes(&subrecord.data);
        }
        writer.len(value.links.len());
        for (raw, link) in &value.links {
            writer.u32(*raw);
            super::save::write_form_id(writer, *link);
        }
    }
}

pub(super) fn read_created_base_section(
    world: &mut World,
    reader: &mut super::save::FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    use soe_papyrus::VmError;
    let count = reader.len()?;
    if count > reader.remaining() / 14 {
        return Err(VmError::State(
            "created base count exceeds remaining bytes".into(),
        ));
    }
    let mut records = BTreeMap::new();
    for _ in 0..count {
        let id = super::save::read_form_id(reader)?;
        if id.file != DYNAMIC_FILE
            || id.object == 0
            || world.order.contains(id)
            || world.record_overrides.contains_key(&id)
        {
            return Err(VmError::State("invalid created base identity".into()));
        }
        let tag = T(&[reader.u8()?, reader.u8()?, reader.u8()?, reader.u8()?]);
        if !matches!(tag.as_bytes(), b"ALCH" | b"ENCH") {
            return Err(VmError::State("invalid created base record type".into()));
        }
        let flags = reader.u32()?;
        let subrecord_count = reader.len()?;
        if subrecord_count > reader.remaining() / 8 {
            return Err(VmError::State(
                "created base subrecord count exceeds remaining bytes".into(),
            ));
        }
        let mut record = Record::new(tag, 0xFF00_0000 | id.object).with_flags(flags);
        for _ in 0..subrecord_count {
            let subtag = T(&[reader.u8()?, reader.u8()?, reader.u8()?, reader.u8()?]);
            record.subrecords.push(soe_esm::plugin::Subrecord::new(
                subtag,
                reader.bytes()?.to_vec(),
            ));
        }
        let link_count = reader.len()?;
        if link_count > reader.remaining() / 10 {
            return Err(VmError::State(
                "created base link count exceeds remaining bytes".into(),
            ));
        }
        let mut value = RecordOverride::new(id, record);
        let used = super::record_overrides::raw_words(&value.record);
        for _ in 0..link_count {
            let raw = reader.u32()?;
            let link = super::save::read_form_id(reader)?;
            if value.links.insert(raw, link).is_some() {
                return Err(VmError::State("duplicate created base link".into()));
            }
        }
        if value.links.keys().any(|raw| !used.contains(raw)) || records.insert(id, value).is_some()
        {
            return Err(VmError::State("invalid created base record links".into()));
        }
    }
    if records
        .keys()
        .any(|id| world.created_base_records.contains_key(id))
    {
        return Err(VmError::State(
            "duplicate created base section identity".into(),
        ));
    }
    world.created_base_records.extend(records);
    Ok(())
}
