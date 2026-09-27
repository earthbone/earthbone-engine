// SPDX-License-Identifier: GPL-3.0-or-later

//! Arrow and bolt release requests from xEdit AMMO/PROJ and CK bow-shot
//! event records. Flight movement is handled by the host.

use soe_esm::{cursor::Fields, GlobalFormId, Tag};

use super::{Alloc, World};
use crate::combat::{Error, FlightRequest};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const AMMO_NON_BOLT: u32 = 1 << 2;
const AMMO_NON_PLAYABLE: u32 = 1 << 1;
const AMMO_RECORD_NON_PLAYABLE: u32 = 1 << 2;
const PROJ_HITSCAN: u16 = 1;
const PROJ_ARROW: u16 = 1 << 6;

/// Bow and crossbow equipment selected for a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangedEquipment {
    /// Equipped bow or crossbow FormID.
    pub weapon: GlobalFormId,
    /// Compatible equipped AMMO FormID.
    pub ammunition: GlobalFormId,
}

impl World {
    /// Resolve a right-hand bow or crossbow and its single equipped ammunition form.
    ///
    /// # Errors
    /// The actor has no ranged weapon, compatible equipped ammunition, or valid records.
    pub fn ranged_equipment(&mut self, actor: GlobalFormId) -> Result<RangedEquipment, Error> {
        let weapon = self
            .hand_weapon(actor, crate::combat::Hand::Right)
            .ok_or(Error::Equipment)?;
        let (_, weapon_record) = self.combat_weapon_source(weapon)?;
        if !matches!(weapon_record.animation, 7 | 9) {
            return Err(Error::Unsupported("weapon is not a bow or crossbow"));
        }
        let required_bolt = weapon_record.animation == 9;
        let equipped = self
            .actor_ref(actor)
            .ok_or(Error::Actor(actor))?
            .equipped
            .clone();
        let mut match_ammo = None;
        for item in equipped {
            if self
                .record(item)
                .is_none_or(|record| record.tag != T(b"AMMO") || record.is_deleted())
            {
                continue;
            }
            let record = self.record(item).ok_or(Error::Equipment)?;
            let data = record.required(T(b"DATA"))?;
            if data.len() < 20 {
                return Err(soe_esm::Error::Invalid(format!("short AMMO DATA for {item}")).into());
            }
            let Some(bytes) = data.get(4..8) else {
                return Err(Error::Equipment);
            };
            let flags = u32::from_le_bytes(bytes.try_into().map_err(|_| Error::Equipment)?);
            let is_bolt = flags & AMMO_NON_BOLT == 0;
            if is_bolt != required_bolt || self.item_count(actor, item) <= 0 {
                continue;
            }
            if match_ammo.replace(item).is_some() {
                return Err(Error::Unsupported(
                    "multiple compatible ammunition forms are equipped",
                ));
            }
        }
        let ammunition = match_ammo.ok_or(Error::Equipment)?;
        Ok(RangedEquipment { weapon, ammunition })
    }

    /// Validate and consume one equipped arrow or bolt, returning its flight inputs.
    ///
    /// # Errors
    /// Invalid equipment, projectile records or launch vectors leave ammunition unchanged.
    pub fn release_projectile(
        &mut self,
        actor: GlobalFormId,
        expected_weapon: GlobalFormId,
        origin: [f32; 3],
        direction: [f32; 3],
        alloc: Alloc<'_>,
    ) -> Result<FlightRequest, Error> {
        if !origin.iter().all(|value| value.is_finite()) {
            return Err(Error::Number);
        }
        let direction_length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if !direction_length.is_finite() || direction_length <= f32::EPSILON {
            return Err(Error::Number);
        }
        let equipment = self.ranged_equipment(actor)?;
        if equipment.weapon != expected_weapon {
            return Err(Error::Equipment);
        }
        let ammo_record = self.record(equipment.ammunition).ok_or(Error::Equipment)?;
        let ammo_record_playable = ammo_record.flags & AMMO_RECORD_NON_PLAYABLE == 0;
        let mut ammo_data = Fields::new(
            ammo_record.tag,
            T(b"DATA"),
            ammo_record.required(T(b"DATA"))?,
        );
        let raw_projectile = ammo_data.u32()?;
        let ammo_flags = ammo_data.u32()?;
        let damage = ammo_data.f32()?;
        if !damage.is_finite() || damage < 0.0 {
            return Err(Error::Number);
        }
        let projectile = self
            .resolve_from(equipment.ammunition, raw_projectile)
            .ok_or(Error::Equipment)?;
        let projectile_record = self
            .record(projectile)
            .filter(|record| record.tag == T(b"PROJ") && !record.is_deleted())
            .ok_or(Error::Equipment)?;
        let mut projectile_data = Fields::new(
            projectile_record.tag,
            T(b"DATA"),
            projectile_record.required(T(b"DATA"))?,
        );
        let flags = projectile_data.u16()?;
        let kind = projectile_data.u16()?;
        let gravity = projectile_data.f32()?;
        let speed = projectile_data.f32()?;
        let range = projectile_data.f32()?;
        if flags & PROJ_HITSCAN != 0 || kind & PROJ_ARROW == 0 {
            return Err(Error::Unsupported(
                "ammunition does not use an arrow projectile",
            ));
        }
        if !gravity.is_finite()
            || !speed.is_finite()
            || speed <= 0.0
            || !range.is_finite()
            || range <= 0.0
        {
            return Err(Error::Number);
        }
        let selected = self.select_ammunition(actor, equipment.ammunition)?;
        self.remove_selected_notified(actor, &selected, None, alloc)?;
        let direction = direction.map(|value| value / direction_length);
        Ok(FlightRequest {
            attacker: actor,
            weapon: equipment.weapon,
            ammunition: equipment.ammunition,
            projectile,
            origin,
            direction,
            gravity,
            speed,
            range,
            damage,
            recoverable: ammo_record_playable
                && ammo_flags & AMMO_NON_PLAYABLE == 0
                && ammo_flags & AMMO_NON_BOLT != 0,
        })
    }

    /// Create one placed AMMO reference after a missed arrow hits the world.
    ///
    /// The reference uses the existing dynamic identity and placement state,
    /// so the ordinary world save retains its ammunition base and location.
    #[must_use]
    pub fn place_recovered_arrow(
        &mut self,
        request: &crate::combat::FlightRequest,
        position: [f32; 3],
    ) -> Option<soe_esm::GlobalFormId> {
        if !request.recoverable || !position.iter().all(|value| value.is_finite()) {
            return None;
        }
        let id = self.place_at(request.ammunition, request.attacker, false);
        let reference = self.refs.get_mut(&id)?;
        reference.position = position;
        Some(id)
    }

    fn select_ammunition(
        &mut self,
        actor: GlobalFormId,
        ammunition: GlobalFormId,
    ) -> Result<crate::inventory::Selection, Error> {
        self.reference_ref(actor)
            .ok_or(Error::Actor(actor))?
            .inventory
            .select_any(ammunition, 1)
            .map_err(Error::from)
    }
}
