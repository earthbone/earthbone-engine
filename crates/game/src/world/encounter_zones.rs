// SPDX-License-Identifier: GPL-3.0-or-later

//! Encounter-zone levels from xEdit ECZN/XEZN and CK Encounter Zone.
//! Installed SkyrimSE getters and initialization establish scaling and locking.

use std::collections::BTreeMap;

use soe_esm::{Error, GlobalFormId};
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::{Value, VmError};

use super::{le_u32, World, T};

impl World {
    /// Resolve reference, parent-cell and worldspace encounter zones in that order.
    /// Reads existing placement without initializing a reference or rolling actors.
    ///
    /// # Errors
    /// Missing references, malformed links or links to non-zone records.
    pub fn encounter_zone(&self, reference: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        if !self.refs.contains_key(&reference)
            && self
                .record(reference)
                .is_none_or(|record| record.tag != T(b"REFR") && record.tag != T(b"ACHR"))
        {
            return Err(invalid("missing encounter-level reference"));
        }
        if let Some(zone) = self.zone_link(reference)? {
            return Ok(Some(zone));
        }
        let cell = if let Some(state) = self.refs.get(&reference) {
            state.cell
        } else if let Some(record) = self.record(reference) {
            if record.tag != T(b"REFR") && record.tag != T(b"ACHR") {
                return Err(invalid("encounter-level input must be a reference"));
            }
            record
                .placement
                .cell
                .and_then(|raw| self.resolve_from(reference, raw))
        } else {
            return Err(invalid("missing encounter-level reference"));
        };
        self.zone_for_cell(cell)
    }

    /// Read a receiver's encounter level without the optional vanilla variance.
    /// An unlocked zone is calculated without locking. Missing zones and NoZoneZone
    /// use current player level. The result is at least one, with no player clamp.
    ///
    /// # Errors
    /// Invalid references, zone records or scaling settings.
    pub fn encounter_level(&self, reference: GlobalFormId) -> Result<u32, Error> {
        self.level_for_zone(self.encounter_zone(reference)?)
    }

    /// Lock a zone on its first entry. Later calls retain the saved level.
    /// Runtime zero denotes an unlocked zone and is never saved as a lock.
    ///
    /// # Errors
    /// Invalid zone records, player level or scaling settings.
    pub fn enter_encounter_zone(&mut self, zone: GlobalFormId) -> Result<u32, Error> {
        if self.no_zone(zone) {
            return self.player_encounter_level();
        }
        let level = if let Some(level) = self.encounter_levels.get(&zone) {
            *level
        } else {
            let level = self.calculate_zone_level(zone)?;
            if level != 0 {
                self.encounter_levels.insert(zone, level);
            }
            level
        };
        Ok(u32::from(level).max(1))
    }

    fn no_zone(&self, zone: GlobalFormId) -> bool {
        zone.object == 0x1e && self.order.position_of("Skyrim.esm") == Some(usize::from(zone.file))
    }

    fn zone_link(&self, source: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        let Some(record) = self.record(source) else {
            return Ok(None);
        };
        let Some(bytes) = record.find(T(b"XEZN")) else {
            return Ok(None);
        };
        if bytes.len() != 4 {
            return Err(invalid("invalid XEZN length"));
        }
        let Some(raw) = le_u32(bytes).filter(|raw| *raw != 0) else {
            return Ok(None);
        };
        let zone = self
            .resolve_from(source, raw)
            .ok_or_else(|| invalid("unresolved XEZN"))?;
        if !self.no_zone(zone) {
            self.zone_data(zone)?;
        }
        Ok(Some(zone))
    }

    fn zone_for_cell(&self, cell: Option<GlobalFormId>) -> Result<Option<GlobalFormId>, Error> {
        let Some(cell) = cell else { return Ok(None) };
        let record = self
            .record(cell)
            .filter(|record| record.tag == T(b"CELL") && !record.is_deleted())
            .ok_or_else(|| invalid("missing encounter-zone cell"))?;
        if let Some(zone) = self.zone_link(cell)? {
            return Ok(Some(zone));
        }
        let world = record
            .placement
            .world
            .and_then(|raw| self.resolve_from(cell, raw));
        match world {
            Some(world) => {
                if self
                    .record(world)
                    .is_none_or(|record| record.tag != T(b"WRLD") || record.is_deleted())
                {
                    return Err(invalid("missing encounter-zone worldspace"));
                }
                self.zone_link(world)
            }
            None => Ok(None),
        }
    }

    pub(super) fn enter_cell_zone(&mut self, cell: GlobalFormId) -> Result<(), Error> {
        if let Some(zone) = self.zone_for_cell(Some(cell))? {
            self.enter_encounter_zone(zone)?;
        }
        Ok(())
    }

    pub(super) fn encounter_level_in_cell(&self, cell: Option<GlobalFormId>) -> Result<u32, Error> {
        self.level_for_zone(self.zone_for_cell(cell)?)
    }

    fn level_for_zone(&self, zone: Option<GlobalFormId>) -> Result<u32, Error> {
        let Some(zone) = zone.filter(|zone| !self.no_zone(*zone)) else {
            return self.player_encounter_level();
        };
        let level = self
            .encounter_levels
            .get(&zone)
            .copied()
            .map_or_else(|| self.calculate_zone_level(zone), Ok)?;
        Ok(u32::from(level).max(1))
    }

    fn player_encounter_level(&self) -> Result<u32, Error> {
        if self.player_level > u32::from(u16::MAX) {
            return Err(invalid("player level exceeds runtime range"));
        }
        Ok(self.player_level.max(1))
    }

    fn zone_data(&self, zone: GlobalFormId) -> Result<(u8, u8, u8), Error> {
        let record = self
            .record(zone)
            .filter(|r| r.tag == T(b"ECZN") && !r.is_deleted())
            .ok_or_else(|| invalid("missing encounter zone"))?;
        match record.find(T(b"DATA")) {
            Some(data) if data.len() == 12 => Ok((data[9], data[11], data[10])),
            Some(data) if data.len() == 8 => Ok((0, 0, 0)),
            _ => Err(invalid("invalid ECZN DATA length")),
        }
    }

    fn calculate_zone_level(&self, zone: GlobalFormId) -> Result<u16, Error> {
        let (minimum, maximum, flags) = self.zone_data(zone)?;
        let player = self.player_encounter_level()?;
        let minimum = u32::from(minimum);
        let mut level = if player < minimum {
            if flags & 2 == 0 {
                minimum
            } else {
                player
            }
        } else {
            let multiplier = match self.game_setting("fLevelScalingMult") {
                Some(Value::Float(value)) => value,
                None if self.form("fLevelScalingMult").is_none() => 0.6,
                _ => return Err(invalid("invalid fLevelScalingMult")),
            };
            let scaled = minimum as f32 + (player - minimum) as f32 * multiplier;
            if !multiplier.is_finite()
                || !scaled.is_finite()
                || !(0.0..=f32::from(u16::MAX)).contains(&scaled)
            {
                return Err(invalid("encounter level exceeds runtime range"));
            }
            scaled as u32
        };
        if maximum != 0 {
            level = level.min(u32::from(maximum));
        }
        Ok(level as u16)
    }

    pub(super) fn write_encounter_levels(&self, out: &mut Writer) {
        out.len(self.encounter_levels.len());
        for (zone, level) in &self.encounter_levels {
            super::save::write_form_id(out, *zone);
            out.u16(*level);
        }
    }

    pub(super) fn read_encounter_levels(
        &self,
        input: &mut Reader<'_>,
    ) -> Result<BTreeMap<GlobalFormId, u16>, VmError> {
        let mut levels = BTreeMap::new();
        for _ in 0..input.len()? {
            let zone = super::save::read_form_id(input)?;
            let level = input.u16()?;
            if self.no_zone(zone)
                || self.zone_data(zone).is_err()
                || level == 0
                || levels.insert(zone, level).is_some()
            {
                return Err(VmError::State(
                    "invalid or duplicate encounter-zone lock".into(),
                ));
            }
        }
        Ok(levels)
    }
}

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
