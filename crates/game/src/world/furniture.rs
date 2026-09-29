// SPDX-License-Identifier: GPL-3.0-or-later

//! Furniture marker selection and actor reservations. The marker layout
//! follows local xEdit `wbRecord(FURN)` and `uesp-modfileformat/FURN.wikitext`.
//! Reservation meaning follows the CK `IsFurnitureInUse` and
//! `IsFurnitureMarkerInUse` contracts.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{World, T};

const SIT_MARKER: u16 = 0x0001;

pub(crate) fn write_furniture_seat(state: &super::ActorState, writer: &mut Writer) {
    writer.bool(state.furniture_seat.is_some());
    if let Some((furniture, marker)) = state.furniture_seat {
        write_form_id(writer, furniture);
        writer.u16(marker);
    }
}

pub(crate) fn read_furniture_seat(
    state: &mut super::ActorState,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    if reader.bool()? {
        state.furniture_seat = Some((read_form_id(reader)?, reader.u16()?));
    }
    Ok(())
}

impl World {
    /// The furniture marker an actor currently reserves, if seated.
    #[must_use]
    pub fn furniture_seat(&self, actor: GlobalFormId) -> Option<(GlobalFormId, u16)> {
        self.actors
            .get(&actor)
            .and_then(|state| state.furniture_seat)
    }

    /// Seat an actor at the first enabled, unreserved Sit marker on furniture.
    /// Activating the same furniture again makes the actor stand.
    ///
    /// Returns `false` for a missing actor or reference and when no Sit marker
    /// can be selected.
    pub fn activate_furniture(&mut self, furniture: GlobalFormId, actor: GlobalFormId) -> bool {
        if self
            .actors
            .get(&actor)
            .and_then(|state| state.furniture_seat)
            .is_some_and(|(current, _)| current == furniture)
        {
            return self.stand_from_furniture(actor);
        }

        let Some((base, position, rotation, enabled, deleted)) =
            self.reference(furniture).map(|reference| {
                (
                    reference.base,
                    reference.position,
                    reference.rotation,
                    reference.enabled,
                    reference.deleted,
                )
            })
        else {
            return false;
        };
        let Some(base) = base.filter(|base| self.tag_of(*base) == Some(T(b"FURN"))) else {
            return false;
        };
        if !enabled || deleted || !self.is_actor_reference(actor) {
            return false;
        }
        let Some(record) = self.record(base) else {
            return false;
        };
        let active_markers = record
            .find(T(b"MNAM"))
            .filter(|data| data.len() >= 4)
            .map(|data| u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x00ff_ffff)
            .unwrap_or(0);
        let occupied: std::collections::BTreeSet<_> = self
            .actors
            .values()
            .filter_map(|state| state.furniture_seat)
            .filter_map(|(reference, marker)| (reference == furniture).then_some(marker))
            .collect();
        let marker = record
            .all(T(b"FNPR"))
            .enumerate()
            .find_map(|(index, data)| {
                let raw = data.get(..2)?;
                let kind = u16::from_le_bytes([raw[0], raw[1]]);
                let marker = u16::try_from(index).ok()?;
                (index < 24
                    && kind == SIT_MARKER
                    && active_markers & (1 << index) != 0
                    && !occupied.contains(&marker))
                .then_some(marker)
            });
        let Some(marker) = marker else {
            return false;
        };
        let Some(state) = self.actor(actor) else {
            return false;
        };
        state.furniture_seat = Some((furniture, marker));
        if let Some(reference) = self.refs.get_mut(&actor) {
            reference.position = position;
            reference.rotation = rotation;
        }
        true
    }

    /// Release an actor's furniture marker reservation when it stands.
    pub fn stand_from_furniture(&mut self, actor: GlobalFormId) -> bool {
        let Some(state) = self.actors.get_mut(&actor) else {
            return false;
        };
        state.furniture_seat.take().is_some()
    }

    /// Whether an actor currently reserves a marker on this furniture.
    #[must_use]
    pub fn furniture_marker_in_use(&self, furniture: GlobalFormId, marker: Option<u16>) -> bool {
        self.actors.values().any(|state| {
            state.furniture_seat.is_some_and(|(reference, index)| {
                reference == furniture && marker.is_none_or(|marker| marker == index)
            })
        })
    }

    fn is_actor_reference(&mut self, actor: GlobalFormId) -> bool {
        self.reference(actor)
            .is_some_and(|reference| reference.is_actor)
    }
}
