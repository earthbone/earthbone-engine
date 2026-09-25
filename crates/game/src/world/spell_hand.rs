// SPDX-License-Identifier: GPL-3.0-or-later

//! Known-spell equip state for the two casting hands. Hand source values
//! follow the CK wiki EquipSpell/UnequipSpell contract, 0 for left and 1 for
//! right. Only SPIT spell type 0 ("Spell" in xEdit's enum) occupies a hand,
//! Powers, Lesser Powers, Abilities and the rest of the type enum belong to
//! their own slots and are rejected here.

use super::World;
use crate::combat::{Error, Hand};
use soe_esm::GlobalFormId;

impl World {
    /// Assign a known spell to a casting hand, replacing any spell already
    /// held there. The opposite hand is untouched.
    ///
    /// # Errors
    /// Rejects a missing actor, a spell the actor does not know, a missing
    /// or malformed SPEL record, and any spell type other than the castable
    /// "Spell" type.
    pub fn equip_hand_spell(
        &mut self,
        actor: GlobalFormId,
        hand: Hand,
        spell: GlobalFormId,
    ) -> Result<(), Error> {
        if !self
            .actor(actor)
            .ok_or(Error::Actor(actor))?
            .spells
            .contains(&spell)
        {
            return Err(Error::Equipment);
        }
        if self
            .spell(spell)
            .map_err(|_| Error::Equipment)?
            .data
            .spell_type
            != 0
        {
            return Err(Error::Equipment);
        }
        let state = self.actor(actor).ok_or(Error::Actor(actor))?;
        match hand {
            Hand::Left => state.left_spell = Some(spell),
            Hand::Right => state.right_spell = Some(spell),
        }
        Ok(())
    }

    /// Read the spell explicitly assigned to a hand, if the actor still
    /// knows it. Stale entries from a spell the actor no longer has read
    /// back as unequipped, matching `weapon_hands::hand_weapon`.
    pub fn hand_spell(&self, actor: GlobalFormId, hand: Hand) -> Option<GlobalFormId> {
        let state = self.actor_ref(actor)?;
        let spell = match hand {
            Hand::Left => state.left_spell,
            Hand::Right => state.right_spell,
        }?;
        state.spells.contains(&spell).then_some(spell)
    }
}
