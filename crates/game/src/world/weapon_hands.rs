// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared hand equipment from WEAP ETYP, EQUP parent rules and DOBJ hand roots.
//! Layouts and identities follow local xEdit and UESP WEAP/EQUP/DOBJ references.

use super::World;
use crate::combat::{Error, Hand};
use soe_esm::{cursor::Fields, GlobalFormId, Tag};

impl World {
    fn weapon_hand_slots(&self, item: GlobalFormId) -> Result<u8, Error> {
        let (item, _) = self.combat_weapon_source(item)?;
        let record = self.record(item).ok_or(Error::Equipment)?;
        let tag = Tag(*b"ETYP");
        let mut field = Fields::new(record.tag, tag, record.required(tag)?);
        field.exact(4)?;
        let equip = self
            .resolve_from(item, field.u32()?)
            .ok_or(Error::Equipment)?;
        let mut left = None;
        let mut right = None;
        for (file, loaded) in self.order.files().iter().enumerate() {
            for record in loaded.plugin.records_of(Tag(*b"DOBJ")) {
                if record.is_deleted() {
                    continue;
                }
                let tag = Tag(*b"DNAM");
                let mut fields = Fields::new(record.tag, tag, record.required(tag)?);
                while fields.remaining() != 0 {
                    let key = fields.u32()?;
                    let raw = fields.u32()?;
                    if raw == 0 {
                        continue;
                    }
                    let form = self.order.resolve(file, raw);
                    if key == u32::from_le_bytes(*b"LHEQ") {
                        left = Some(form);
                    }
                    if key == u32::from_le_bytes(*b"RHEQ") {
                        right = Some(form);
                    }
                }
            }
        }
        let roots = (
            left.ok_or(Error::Equipment)?,
            right.ok_or(Error::Equipment)?,
        );
        if roots.0 == roots.1 {
            return Err(Error::Equipment);
        }
        self.equipment_slot_choices(equip, roots, &mut Vec::new())
    }

    fn equipment_slot_choices(
        &self,
        slot: GlobalFormId,
        roots: (GlobalFormId, GlobalFormId),
        path: &mut Vec<GlobalFormId>,
    ) -> Result<u8, Error> {
        let record = self
            .record(slot)
            .filter(|record| record.tag == Tag(*b"EQUP") && !record.is_deleted())
            .ok_or(Error::Equipment)?;
        if slot == roots.0 {
            return Ok(1 << 1);
        }
        if slot == roots.1 {
            return Ok(1 << 2);
        }
        if path.len() >= 64 || path.contains(&slot) {
            return Err(Error::Unsupported("cyclic equipment slot parents"));
        }
        let tag = Tag(*b"DATA");
        let mut field = Fields::new(record.tag, tag, record.required(tag)?);
        field.exact(4)?;
        let all = match field.u32()? {
            0 => false,
            1 => true,
            _ => return Err(Error::Unsupported("equipment slot parent mode")),
        };
        let tag = Tag(*b"PNAM");
        let mut parents = Fields::new(record.tag, tag, record.required(tag)?);
        if parents.remaining() == 0 {
            return Err(Error::Equipment);
        }
        path.push(slot);
        let mut choices = u8::from(all);
        while parents.remaining() != 0 {
            let parent = self
                .resolve_from(slot, parents.u32()?)
                .ok_or(Error::Equipment)?;
            let next = self.equipment_slot_choices(parent, roots, path)?;
            if all {
                let mut combined = 0;
                for a in 0..4 {
                    for b in 1..4 {
                        if choices & (1 << a) != 0 && next & (1 << b) != 0 {
                            combined |= 1 << (a | b);
                        }
                    }
                }
                choices = combined;
            } else {
                choices |= next;
            }
        }
        path.pop();
        Ok(choices)
    }

    /// Select a weapon in the shared equipment state for a specific hand.
    /// Two-handed melee weapons occupy both hands and are selected by the right.
    ///
    /// # Errors
    /// Missing actor/item, malformed weapon, incompatible hand or insufficient copies.
    pub fn equip_hand_weapon(
        &mut self,
        actor: GlobalFormId,
        hand: Hand,
        item: GlobalFormId,
    ) -> Result<(), Error> {
        let (_, weapon) = self.combat_weapon_source(item)?;
        if !matches!(weapon.animation, 1..=6) {
            return Err(Error::Unsupported("non-melee hand weapon"));
        }
        let slots = self.weapon_hand_slots(item)?;
        let selected = match hand {
            Hand::Left => 1,
            Hand::Right => 2,
        };
        let two_handed = slots & (1 << selected) == 0 && slots & (1 << 3) != 0;
        if slots & ((1 << selected) | (1 << 3)) == 0 {
            return Err(Error::Equipment);
        }
        if two_handed && hand == Hand::Left {
            return Err(Error::Equipment);
        }
        let count = self.item_count(actor, item);
        let state = self.actor(actor).ok_or(Error::Actor(actor))?;
        let other = match hand {
            Hand::Left => state.right_weapon,
            Hand::Right => state.left_weapon,
        };
        if count < 1 + i32::from(other == Some(item) && !two_handed) {
            return Err(Error::Equipment);
        }
        let mut left = state.left_weapon;
        let mut right = state.right_weapon;
        if hand == Hand::Left {
            if let Some(right) = right {
                let slots = self.weapon_hand_slots(right)?;
                if slots & (1 << 2) == 0 && slots & (1 << 3) != 0 {
                    return Err(Error::Equipment);
                }
            }
            left = Some(item);
        } else {
            right = Some(item);
            if two_handed {
                left = None;
            }
        }
        let state = self.actor(actor).ok_or(Error::Actor(actor))?;
        for old in [state.left_weapon, state.right_weapon]
            .into_iter()
            .flatten()
        {
            if Some(old) != left && Some(old) != right {
                state.equipped.remove(&old);
            }
        }
        state.left_weapon = left;
        state.right_weapon = right;
        state.equipped.insert(item);
        Ok(())
    }

    /// Read an explicitly assigned weapon, rejecting stale inventory/equipment.
    /// Older saves have no hand assignment until equipment is selected again.
    pub fn hand_weapon(&mut self, actor: GlobalFormId, hand: Hand) -> Option<GlobalFormId> {
        let state = self.actor(actor)?;
        let item = match hand {
            Hand::Left => state.left_weapon,
            Hand::Right => state.right_weapon,
        }?;
        if !state.equipped.contains(&item) {
            return None;
        }
        let copies =
            1 + i32::from(state.left_weapon == Some(item) && state.right_weapon == Some(item));
        (self.item_count(actor, item) >= copies).then_some(item)
    }
}
