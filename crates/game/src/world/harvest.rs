// SPDX-License-Identifier: GPL-3.0-or-later

//! FLOR/TREE harvesting follows xEdit PFIG, SNAM and PFPC and CK Flora.
//! Harvest timestamps live in the shared world and use the cell respawn GMSTs.

use super::{Alloc, World};
use soe_esm::{Error, GlobalFormId, Tag};
use soe_papyrus::{
    vm::state::{ArrayPool, ArrayTable, Writer},
    Value, VmError,
};
use std::collections::BTreeMap;

/// A winning flora or tree's ingredient production and activation text.
pub struct HarvestData {
    pub produce: GlobalFormId,
    pub sound: Option<GlobalFormId>,
    pub chances: [u8; 4],
    pub verb: String,
}

impl World {
    /// Read one harvestable base without changing reference state.
    /// Invalid fields return the plugin reader's typed error.
    pub fn harvest_data(&self, base: GlobalFormId) -> Result<Option<HarvestData>, Error> {
        let Some(record) = self.record(base).filter(|record| {
            !record.is_deleted() && matches!(record.tag.as_bytes(), b"FLOR" | b"TREE")
        }) else {
            return Ok(None);
        };
        let form = |field| -> Result<Option<GlobalFormId>, Error> {
            let Some(raw) = record.find(Tag(field)) else {
                return Ok(None);
            };
            let bytes: [u8; 4] = raw
                .try_into()
                .map_err(|_| Error::Invalid("harvest FormID must have four bytes".into()))?;
            let raw = u32::from_le_bytes(bytes);
            Ok((raw != 0).then(|| self.resolve_from(base, raw)).flatten())
        };
        let Some(produce) = form(*b"PFIG")? else {
            return Ok(None);
        };
        let chances: [u8; 4] = record
            .required(Tag(*b"PFPC"))?
            .try_into()
            .map_err(|_| Error::Invalid("PFPC must contain four seasonal chances".into()))?;
        if chances.iter().any(|chance| *chance > 100) {
            return Err(Error::Invalid("PFPC chance exceeds 100".into()));
        }
        let verb = record
            .find(Tag(*b"RNAM"))
            .map(|raw| {
                self.display_text(
                    base,
                    Tag(*b"RNAM"),
                    &super::text(raw, self.record_text_localized(base, Tag(*b"RNAM"))),
                )
            })
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| match self.game_setting("sHarvest") {
                Some(Value::Str(text)) => text.to_string(),
                _ => "Harvest".into(),
            });
        Ok(Some(HarvestData {
            produce,
            sound: form(*b"SNAM")?,
            chances,
            verb,
        }))
    }

    fn harvest_reset_days(&self, reference: GlobalFormId) -> f64 {
        let cleared = self
            .editor_location(reference)
            .and_then(|location| self.locations.get(&location))
            .is_some_and(|location| location.cleared);
        let (setting, fallback) = if cleared {
            ("iHoursToRespawnCellCleared", 720)
        } else {
            ("iHoursToRespawnCell", 240)
        };
        let hours = match self.game_setting(setting) {
            Some(Value::Int(hours)) if hours >= 0 => hours,
            _ => fallback,
        };
        f64::from(hours) / 24.0
    }

    /// Whether the reference is waiting for its current respawn interval.
    pub fn harvested(&self, reference: GlobalFormId) -> bool {
        self.harvested_at
            .get(&reference)
            .is_some_and(|time| self.clock.game_days - time < self.harvest_reset_days(reference))
    }

    /// The original model whose switch nodes must show the harvested state.
    pub fn harvested_model(&self, reference: GlobalFormId) -> Option<String> {
        if !self.harvested(reference) {
            return None;
        }
        let base = self.refs.get(&reference)?.base?;
        let model = soe_esm::cursor::zstring(self.record(base)?.find(Tag(*b"MODL"))?);
        (!model.is_empty()).then_some(model)
    }

    pub(super) fn harvest_reference(
        &mut self,
        reference: GlobalFormId,
        actor: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<(), Error> {
        let Some(base) = self.reference(reference).and_then(|state| state.base) else {
            return Ok(());
        };
        let Some(data) = self.harvest_data(base)? else {
            return Ok(());
        };
        if self.harvested(reference) {
            return Ok(());
        }
        if self.actor(actor).is_none() {
            return Err(Error::Invalid("flora harvester is not an actor".into()));
        }
        let owner = if self.loose_item_is_stealing(reference, actor) {
            self.loot_owner(reference)
        } else {
            None
        };
        let previous_rng = self.rng;
        let previous_streams = self.rng_streams;
        let result = (|| -> Result<_, Error> {
            let season = ((self.calendar().month + 10) % 12 / 3) as usize;
            let chance = data.chances[season];
            let success =
                chance == 100 || (chance != 0 && self.random_int(0, 99) < i32::from(chance));
            let mut selected = crate::inventory::Inventory::default();
            if success {
                if self.tag_of(data.produce) == Some(Tag(*b"LVLI")) {
                    let level = self.encounter_level(actor)?;
                    selected = self.roll_leveled_inventory_context(
                        data.produce,
                        level,
                        1,
                        Some(self.player_level),
                    )?;
                } else {
                    if !self.is_inventory_base(data.produce) {
                        return Err(Error::Invalid("PFIG is not an inventory item".into()));
                    }
                    selected
                        .add(data.produce, 1)
                        .map_err(|error| Error::Invalid(error.to_string()))?;
                }
            }
            self.receive_created_items(actor, selected, owner, alloc)
                .ok_or_else(|| Error::Invalid("harvest inventory transaction failed".into()))
        })();
        let changes = match result {
            Ok(changes) => changes,
            Err(error) => {
                self.rng = previous_rng;
                self.rng_streams = previous_streams;
                return Err(error);
            }
        };
        self.harvested_at.insert(reference, self.clock.game_days);
        self.invalidate_reference(reference);
        if let Some(sound) = data.sound {
            self.play_sound(sound, reference);
        }
        for (produce, _) in changes {
            self.commonlib_harvested(produce, actor);
        }
        Ok(())
    }

    pub(super) fn expire_harvests(&mut self) {
        let expired: Vec<_> = self
            .harvested_at
            .keys()
            .copied()
            .filter(|id| !self.harvested(*id))
            .collect();
        for id in expired {
            self.harvested_at.remove(&id);
            self.invalidate_reference(id);
        }
    }
}

pub(super) fn write_harvests(world: &World, output: &mut Writer, _pool: &mut ArrayPool) {
    output.len(world.harvested_at.len());
    for (id, time) in &world.harvested_at {
        super::save::write_form_id(output, *id);
        output.f64(*time);
    }
}

pub(super) fn read_harvests(
    world: &mut World,
    input: &mut super::save::FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let mut harvested = BTreeMap::new();
    for _ in 0..input.len()? {
        let id = super::save::read_form_id(input)?;
        let time = input.f64()?;
        if !time.is_finite() || harvested.insert(id, time).is_some() {
            return Err(VmError::State(
                "invalid or duplicate INV-09 harvest timestamp".into(),
            ));
        }
    }
    world.harvested_at = harvested;
    Ok(())
}
