// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared placement of the actors a host contact query may strike, and the
//! reach of a weapon swing. The World owns both. A host presents the
//! placements as physics bodies and rebuilds them from this list whenever
//! the loaded cells change, so no host node is needed per reference.

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use super::World;
use crate::combat::{records::Weapon, Error, Hand};

/// Vanilla `fCombatDistance` in game units. UESP's WEAP page gives melee
/// reach as `fCombatDistance * NPCScale * WeaponReach`, and skymp's
/// `GetReach` multiplies the same setting by the DNAM reach. Used only when
/// the load order carries no such GMST.
pub const DEFAULT_COMBAT_DISTANCE: f32 = 141.0;

/// One actor a contact query can strike, in game units and radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactActor {
    pub actor: GlobalFormId,
    pub cell: GlobalFormId,
    /// The reference's feet position.
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Controller scale: race height times NPC height times reference scale.
    pub scale: f32,
}

impl World {
    /// Every resident actor reference that stands enabled in a resident cell,
    /// sorted by form. A cell is resident when a transition loaded it or the
    /// exterior ring streamed it in, see [`World::cell_is_resident`]. Loading
    /// a cell builds its references, so an actor needs no run-time state yet
    /// to be listed. Dead actors stay listed because a corpse still blocks a
    /// blade.
    ///
    /// An actor whose controller scale cannot be derived keeps scale 1.0
    /// times its reference scale, so a template error never drops a body.
    pub fn contact_actors(&mut self) -> Vec<ContactActor> {
        let mut actors: Vec<GlobalFormId> = self
            .refs
            .iter()
            .filter(|(_, reference)| reference.is_actor)
            .map(|(id, _)| *id)
            .collect();
        actors.sort_unstable();
        let mut out = Vec::new();
        for actor in actors {
            let Some(reference) = self.refs.get(&actor) else {
                continue;
            };
            if !reference.enabled || reference.deleted {
                continue;
            }
            let (Some(cell), Some(base)) = (reference.cell, reference.base) else {
                continue;
            };
            if self.loaded_cell != Some(cell) && !self.cell_is_resident(cell) {
                continue;
            }
            let position = reference.position;
            let rotation = reference.rotation;
            let placed = reference.scale;
            if !position
                .iter()
                .chain(rotation.iter())
                .all(|v| v.is_finite())
            {
                continue;
            }
            let race = self.actor_controller_scale(base).unwrap_or(1.0);
            let placed = if placed.is_finite() && placed > 0.0 {
                placed
            } else {
                1.0
            };
            let scale = race * placed;
            let scale = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
            out.push(ContactActor {
                actor,
                cell,
                position,
                rotation,
                scale,
            });
        }
        out
    }

    /// Melee reach in game units for the weapon `attacker` holds in `hand`:
    /// `fCombatDistance` times the weapon's DNAM reach times the reference
    /// scale, the formula UESP documents on the WEAP page.
    ///
    /// # Errors
    /// No weapon in that hand, an unreadable weapon record, an invalid
    /// `fCombatDistance`, or a non-finite result.
    pub fn melee_reach(&mut self, attacker: GlobalFormId, hand: Hand) -> Result<f32, Error> {
        let weapon = self.hand_weapon(attacker, hand).ok_or(Error::Equipment)?;
        let record = self.record(weapon).ok_or(Error::Equipment)?;
        let reach = Weapon::from_record(record)?.reach;
        let distance = match self.game_setting("fCombatDistance") {
            None => DEFAULT_COMBAT_DISTANCE,
            Some(Value::Float(value)) if value.is_finite() && value > 0.0 => value,
            Some(_) => return Err(Error::Setting("fCombatDistance")),
        };
        let scale = self
            .reference_ref(attacker)
            .map_or(1.0, |reference| reference.scale);
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let out = distance * reach * scale;
        if !out.is_finite() || out < 0.0 {
            return Err(Error::Number);
        }
        Ok(out)
    }
}
