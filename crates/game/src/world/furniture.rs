// SPDX-License-Identifier: GPL-3.0-or-later

//! Furniture marker selection and actor reservations. The marker layout
//! follows local xEdit `wbRecord(FURN)` and `uesp-modfileformat/FURN.wikitext`.
//! Reservation meaning follows the CK `IsFurnitureInUse` and
//! `IsFurnitureMarkerInUse` contracts.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::save_sections::Section;
use super::{ActorState, World, T};

const SIT_MARKER: u16 = 0x0001;
const SLEEP_MARKER: u16 = 0x0002;

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

pub(super) const SECTION: Section<ActorState> = Section {
    name: "HST-09",
    write: write_furniture_seat,
    read: read_furniture_seat,
};

impl World {
    /// Whether an enabled sleep marker on furniture can be reserved by the player.
    #[must_use]
    pub fn can_sleep_at_furniture(&mut self, furniture: GlobalFormId) -> bool {
        if !self.clock.timescale.is_finite() || self.clock.timescale <= 0.0 {
            return false;
        }
        let player = self.player();
        self.sleep_marker(furniture).is_some_and(|marker| {
            !self.furniture_marker_in_use(furniture, Some(marker))
                || self.furniture_seat(player) == Some((furniture, marker))
        })
    }

    /// Sleep at an enabled bed marker, advance game time and release the marker
    /// when the player wakes. `elapsed_hours` is the time slept before waking.
    pub fn sleep_at_furniture(
        &mut self,
        furniture: GlobalFormId,
        elapsed_hours: f64,
        interrupted: bool,
    ) -> bool {
        if !elapsed_hours.is_finite()
            || elapsed_hours <= 0.0
            || !self.clock.timescale.is_finite()
            || self.clock.timescale <= 0.0
        {
            return false;
        }
        let Some(marker) = self.sleep_marker(furniture) else {
            return false;
        };
        let player = self.player();
        if self
            .furniture_seat(player)
            .is_some_and(|(current, _)| current != furniture)
            || (self.furniture_marker_in_use(furniture, Some(marker))
                && self.furniture_seat(player) != Some((furniture, marker)))
        {
            return false;
        }
        if self.furniture_seat(player) != Some((furniture, marker)) {
            if self.furniture_seat(player).is_some() {
                self.stand_from_furniture(player);
            }
            let Some((_, position, rotation)) = self
                .reference(furniture)
                .map(|reference| (reference.base, reference.position, reference.rotation))
            else {
                return false;
            };
            if let Some(actor) = self.actor(player) {
                actor.furniture_seat = Some((furniture, marker));
            } else {
                return false;
            }
            if let Some(reference) = self.refs.get_mut(&player) {
                reference.position = position;
                reference.rotation = rotation;
            }
        }
        let start = self.clock.game_days as f32;
        let end = (self.clock.game_days + elapsed_hours / 24.0) as f32;
        self.sleep_start(start, end);
        self.advance_clock(elapsed_hours * 3600.0 / self.clock.timescale);
        self.sleep_stop(interrupted);
        self.stand_from_furniture(player);
        true
    }

    fn sleep_marker(&mut self, furniture: GlobalFormId) -> Option<u16> {
        let (base, enabled, deleted) = self
            .reference(furniture)
            .and_then(|reference| Some((reference.base?, reference.enabled, reference.deleted)))?;
        let player = self.player();
        if !enabled || deleted || !self.is_actor_reference(player) {
            return None;
        }
        let record = self.record(base)?;
        if record.tag != T(b"FURN") {
            return None;
        }
        let active_markers = record
            .find(T(b"MNAM"))
            .filter(|data| data.len() >= 4)
            .map(|data| u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0x00ff_ffff)
            .unwrap_or(0);
        record
            .all(T(b"FNPR"))
            .enumerate()
            .find_map(|(index, data)| {
                if data.len() != 4 {
                    return None;
                }
                let raw = data.get(..2)?;
                let kind = u16::from_le_bytes([raw[0], raw[1]]);
                let marker = u16::try_from(index).ok()?;
                (index < 24 && kind == SLEEP_MARKER && active_markers & (1 << index) != 0)
                    .then_some(marker)
            })
    }

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
