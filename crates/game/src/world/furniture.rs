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

pub(crate) fn write_furniture_seat(
    state: &ActorState,
    writer: &mut Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    writer.bool(state.furniture_seat.is_some());
    if let Some((furniture, marker)) = state.furniture_seat {
        write_form_id(writer, furniture);
        writer.u16(marker);
    }
}

pub(crate) fn read_furniture_seat(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), VmError> {
    if reader.bool()? {
        actor.furniture_seat = Some((read_form_id(reader)?, reader.u16()?));
    }
    Ok(())
}

pub(super) const SECTION: Section<ActorState> = Section {
    name: "HST-09",
    write: write_furniture_seat,
    read: read_furniture_seat,
};

impl World {
    /// Select a condition-matching furniture idle through its IDLE parent chain.
    /// IDLE events and parent links come from winning records, including overrides.
    pub fn furniture_entry_idle(
        &mut self,
        actor: GlobalFormId,
        animation: u16,
        entry: u16,
    ) -> Option<String> {
        use soe_esm::records::gameplay::Condition;
        let (furniture, _) = self.furniture_seat(actor)?;
        let ids: Vec<_> = self
            .order()
            .winners_of(T(b"IDLE"))
            .map(|(id, _)| id)
            .collect();
        let mut matches = Vec::new();
        for id in ids {
            let Some(event) = self
                .record(id)
                .and_then(|record| record.find(T(b"ENAM")))
                .map(soe_esm::cursor::zstring)
                .filter(|event| {
                    !event.is_empty()
                        && event != "IdleFurnitureStart"
                        && event != "IdleFurnitureExit"
                })
            else {
                continue;
            };
            let mut current = Some(id);
            let mut chain = std::collections::BTreeSet::new();
            let mut furniture_branch = false;
            let mut accepted = true;
            while let Some(parent) = current {
                if chain.len() >= 64 || !chain.insert(parent) {
                    accepted = false;
                    break;
                }
                let Some(record) = self
                    .record(parent)
                    .filter(|record| record.tag == T(b"IDLE"))
                else {
                    break;
                };
                furniture_branch |= record
                    .editor_id()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("furniture"));
                let mut conditions = Vec::new();
                if record
                    .subrecords
                    .iter()
                    .try_for_each(|sub| Condition::push(record, sub, &mut conditions).map(|_| ()))
                    .is_err()
                {
                    accepted = false;
                    break;
                }
                furniture_branch |= conditions.iter().any(|condition| condition.function == 613);
                let previous = record
                    .find(T(b"ANAM"))
                    .and_then(|data| data.get(..4))
                    .map(|data| u32::from_le_bytes([data[0], data[1], data[2], data[3]]));
                let context = crate::conditions::Context {
                    owner: parent,
                    subject: actor,
                    target: Some(furniture),
                    quest: None,
                    tempering_enchanted: None,
                    tempering_item: None,
                    alchemy_making_poison: None,
                    alchemy_effect: None,
                    magic_item: None,
                    dual_cast: None,
                    skill_use: None,
                    light_level: None,
                };
                if !crate::conditions::evaluate_furniture(
                    self,
                    context,
                    &conditions,
                    animation,
                    entry,
                )
                .unwrap_or(false)
                {
                    accepted = false;
                    break;
                }
                current = previous
                    .filter(|raw| *raw != 0)
                    .and_then(|raw| self.resolve_from(parent, raw));
            }
            if accepted && furniture_branch {
                matches.push((std::cmp::Reverse(chain.len()), id, event));
            }
        }
        matches.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        matches.into_iter().next().map(|(_, _, event)| event)
    }

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
            self.invalidate_reference(player);
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

        let Some((position, rotation, enabled, deleted)) =
            self.reference(furniture).map(|reference| {
                (
                    reference.position,
                    reference.rotation,
                    reference.enabled,
                    reference.deleted,
                )
            })
        else {
            return false;
        };
        if !enabled || deleted {
            return false;
        }
        if self
            .reserve_furniture_use_marker(furniture, actor)
            .is_none()
        {
            return false;
        }
        self.invalidate_reference(actor);
        if let Some(reference) = self.refs.get_mut(&actor) {
            reference.position = position;
            reference.rotation = rotation;
        }
        true
    }

    /// Reserve the first enabled, unreserved Sit marker without moving the actor.
    ///
    /// Returns its zero-based `FNPR` index, or `None` when the furniture or actor
    /// cannot use a marker.
    pub fn reserve_furniture_marker(
        &mut self,
        furniture: GlobalFormId,
        actor: GlobalFormId,
    ) -> Option<u16> {
        self.reserve_furniture_animation_marker(furniture, actor, SIT_MARKER)
    }

    /// Reserve sitting or leaning furniture for activation and sandbox use.
    pub fn reserve_furniture_use_marker(
        &mut self,
        furniture: GlobalFormId,
        actor: GlobalFormId,
    ) -> Option<u16> {
        self.reserve_furniture_animation_marker(furniture, actor, SIT_MARKER)
            .or_else(|| self.reserve_furniture_animation_marker(furniture, actor, 4))
    }

    /// Reserve an enabled marker with the requested FURN animation type.
    /// Sit, Sleep and Lean are the xEdit FNPR values 1, 2 and 4.
    pub fn reserve_furniture_animation_marker(
        &mut self,
        furniture: GlobalFormId,
        actor: GlobalFormId,
        animation: u16,
    ) -> Option<u16> {
        if !matches!(animation, SIT_MARKER | SLEEP_MARKER | 4) {
            return None;
        }
        if let Some((current, marker)) = self.furniture_seat(actor) {
            if current == furniture {
                return Some(marker);
            }
        }
        let (base, enabled, deleted) = self
            .reference(furniture)
            .map(|reference| (reference.base, reference.enabled, reference.deleted))?;
        let base = base.filter(|base| self.tag_of(*base) == Some(T(b"FURN")))?;
        if !enabled || deleted || !self.is_actor_reference(actor) {
            return None;
        }
        let record = self.record(base)?;
        let flags = record
            .find(T(b"MNAM"))
            .filter(|data| data.len() >= 4)
            .map(|data| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
            .unwrap_or(0);
        if flags & 0x0200_0000 != 0 {
            return None;
        }
        let active_markers = flags & 0x00ff_ffff;
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
                if data.len() != 4 {
                    return None;
                }
                let raw = data.get(..2)?;
                let kind = u16::from_le_bytes([raw[0], raw[1]]);
                let marker = u16::try_from(index).ok()?;
                (index < 24
                    && kind == animation
                    && active_markers & (1 << index) != 0
                    && !occupied.contains(&marker))
                .then_some(marker)
            });
        let marker = marker?;
        let state = self.actor(actor)?;
        state.furniture_seat = Some((furniture, marker));
        Some(marker)
    }

    /// Release an actor's furniture marker reservation when it stands.
    pub fn stand_from_furniture(&mut self, actor: GlobalFormId) -> bool {
        let Some(state) = self.actors.get_mut(&actor) else {
            return false;
        };
        state.sit_wants_to_stand = false;
        state.furniture_seat.take().is_some()
    }

    /// Keep the marker reserved while its authored exit animation plays.
    pub fn request_furniture_exit(&mut self, actor: GlobalFormId) -> bool {
        let Some(state) = self
            .actors
            .get_mut(&actor)
            .filter(|state| state.furniture_seat.is_some())
        else {
            return false;
        };
        if !state.sit_wants_to_stand {
            state.sit_wants_to_stand = true;
            self.request_animation(actor, "IdleFurnitureExit");
        }
        true
    }

    /// Keep a presented marker occupied until its graph has left furniture.
    /// A headless actor without graph state releases its logical reservation.
    pub(crate) fn leave_furniture(&mut self, actor: GlobalFormId) -> bool {
        let presented = self
            .reference_ref(actor)
            .and_then(|reference| reference.anim_ints.get("isinfurniture"))
            .is_some_and(|value| *value != 0);
        if presented && self.request_furniture_exit(actor) {
            true
        } else {
            self.stand_from_furniture(actor);
            false
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(T(b"FURN"), 0x800)
                .sub(b"MNAM", 1_u32.to_le_bytes())
                .sub(b"FNPR", [1, 0, 5, 0]),
        );
        plugin.push_top(Record::new(T(b"REFR"), 0x801).sub(b"NAME", 0x800_u32.to_le_bytes()));
        plugin.push_top(Record::new(T(b"NPC_"), 0x820));
        plugin.push_top(Record::new(T(b"ACHR"), 0x821).sub(b"NAME", 0x820_u32.to_le_bytes()));
        let condition = |function: u16, parameter: u32| {
            let mut bytes = [0_u8; 32];
            bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
            bytes[8..10].copy_from_slice(&function.to_le_bytes());
            bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
            bytes
        };
        plugin.push_top(
            Record::new(T(b"IDLE"), 0x802)
                .zsub(b"EDID", "IdleFurnitureRoot")
                .sub(b"CTDA", condition(613, 1)),
        );
        for (id, entry, event) in [
            (0x803, 0x10000, "IdleChairFrontEnter"),
            (0x804, 0x40000, "IdleChairRightEnter"),
        ] {
            let mut parent = Vec::from(0x802_u32.to_le_bytes());
            parent.extend(0_u32.to_le_bytes());
            plugin.push_top(
                Record::new(T(b"IDLE"), id)
                    .zsub(b"ENAM", event)
                    .sub(b"ANAM", parent)
                    .sub(b"CTDA", condition(614, entry)),
            );
        }
        let mut order = LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let mut world = World::new(order);
        let player = world.player();
        world.reference(player).unwrap().is_actor = true;
        assert_eq!(
            world.reserve_furniture_marker(
                GlobalFormId {
                    file: 0,
                    object: 0x801
                },
                player
            ),
            Some(0)
        );
        world
    }

    #[test]
    fn r4_furniture_idle_matches_parent_type_and_authored_entry_condition() {
        let mut world = world();
        let actor = world.player();
        assert_eq!(
            world.furniture_entry_idle(actor, 1, 1).as_deref(),
            Some("IdleChairFrontEnter")
        );
        assert_eq!(
            world.furniture_entry_idle(actor, 1, 4).as_deref(),
            Some("IdleChairRightEnter")
        );
        assert_eq!(world.furniture_entry_idle(actor, 2, 1), None);
        assert_eq!(world.furniture_entry_idle(actor, 1, 8), None);
    }

    #[test]
    fn r4_furniture_exit_keeps_its_marker_until_playback_finishes() {
        let mut world = world();
        let actor = world.player();
        let (furniture, marker) = world.furniture_seat(actor).unwrap();
        assert!(world.request_furniture_exit(actor));
        assert!(world.actor_ref(actor).unwrap().sit_wants_to_stand);
        assert!(world.furniture_marker_in_use(furniture, Some(marker)));
        assert!(world.stand_from_furniture(actor));
        assert!(!world.furniture_marker_in_use(furniture, Some(marker)));
        assert!(!world.actor_ref(actor).unwrap().sit_wants_to_stand);
    }

    #[test]
    fn r4_presented_exit_excludes_a_second_actor_until_graph_completion() {
        let mut world = world();
        let actor = world.player();
        let other = GlobalFormId {
            file: 0,
            object: 0x821,
        };
        let (furniture, _) = world.furniture_seat(actor).unwrap();
        world
            .reference(actor)
            .unwrap()
            .anim_ints
            .insert("isinfurniture".into(), 1);
        assert!(world.leave_furniture(actor));
        assert!(world.leave_furniture(actor));
        assert_eq!(world.animation_requests.len(), 1);
        assert_eq!(world.animation_requests[0].animation, "IdleFurnitureExit");
        assert_eq!(world.reserve_furniture_marker(furniture, other), None);
        assert_eq!(world.sit_state(actor), Some(4));
        world
            .reference(actor)
            .unwrap()
            .anim_ints
            .insert("isinfurniture".into(), 0);
        assert!(!world.leave_furniture(actor));
        assert_eq!(world.sit_state(actor), Some(0));
        assert_eq!(world.reserve_furniture_marker(furniture, other), Some(0));
        assert_eq!(world.sit_state(other), Some(3));
    }
}
