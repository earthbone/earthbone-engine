// SPDX-License-Identifier: GPL-3.0-or-later

//! Default outfit initialization from xEdit OTFT and the runtime outfit visitor.
//! Selection is separate from AddItem: one tagged instance per selected base,
//! without COED, then instance equipment. Sources and limits are in docs/game.md.

use soe_esm::{Error, GlobalFormId};

use super::{le_u32, World, T};
use crate::inventory::{Extra, Field, Inventory, Stack};
use crate::npc::template;

impl World {
    /// Prepare only a fresh actor's default outfit. No actor/reference lookup or events.
    pub(super) fn default_outfit_inventory(
        &mut self,
        reference: GlobalFormId,
        base: GlobalFormId,
        inventory: &Inventory,
    ) -> Result<Inventory, Error> {
        let previous_rng = self.rng;
        let result = self.prepare_default_outfit(reference, base, inventory);
        if result.is_err() {
            self.rng = previous_rng;
        }
        result
    }

    fn prepare_default_outfit(
        &mut self,
        reference: GlobalFormId,
        base: GlobalFormId,
        inventory: &Inventory,
    ) -> Result<Inventory, Error> {
        let source = self.reference_component_source(reference, base, template::INVENTORY)?;
        let npc = self.npc_record(source)?;
        let Some(raw) = npc.outfit.filter(|raw| *raw != 0) else {
            return Ok(inventory.clone());
        };
        let outfit = self
            .resolve_from(source, raw)
            .ok_or_else(|| bad("unresolved DOFT"))?;
        let record = self
            .record(outfit)
            .filter(|record| record.tag == T(b"OTFT") && !record.is_deleted())
            .ok_or_else(|| bad("missing default outfit"))?;
        let mut entries = Vec::new();
        for bytes in record.all(T(b"INAM")) {
            if bytes.len() % 4 != 0 {
                return Err(bad("invalid outfit INAM length"));
            }
            for bytes in bytes.as_chunks::<4>().0 {
                let raw = le_u32(bytes).ok_or_else(|| bad("invalid outfit item"))?;
                let item = self
                    .resolve_from(outfit, raw)
                    .ok_or_else(|| bad("unresolved outfit item"))?;
                if raw == 0
                    || !(self.tag_of(item) == Some(T(b"ARMO"))
                        || self.tag_of(item) == Some(T(b"LVLI")))
                {
                    return Err(bad("outfit entry must be armor or a leveled item list"));
                }
                entries.push(item);
            }
        }
        let stats = self.reference_component_source(reference, base, template::STATS)?;
        let level = self.npc_record(stats)?.level_for(self.player_level);
        let mut result = inventory.clone();
        for entry in entries {
            let items = if self.tag_of(entry) == Some(T(b"LVLI")) {
                self.roll_outfit_items(entry, level)?
                    .iter()
                    .map(|(item, _)| *item)
                    .collect()
            } else {
                vec![entry]
            };
            for item in items {
                if result.stacks(item).iter().any(|stack| {
                    stack
                        .extra(142)
                        .is_some_and(|extra| extra.fields == [Field::Form(Some(outfit))])
                }) {
                    continue;
                }
                let slots = self.outfit_wear_slots(item)?;
                if let Some(slots) = slots {
                    let conflicts: Vec<_> = result
                        .iter()
                        .filter_map(|(other, _)| {
                            result
                                .stacks(*other)
                                .iter()
                                .any(Stack::worn)
                                .then_some(*other)
                        })
                        .collect();
                    for other in conflicts {
                        if other == item
                            || self.outfit_wear_slots(other)?.is_some_and(|other_slots| {
                                slots & other_slots & ((1 << 14) - 1) != 0
                            })
                        {
                            result.unequip(other);
                        }
                    }
                }
                let mut extras = vec![Extra {
                    kind: 142,
                    fields: vec![Field::Form(Some(outfit))],
                }];
                if slots.is_some() {
                    extras.push(Extra {
                        kind: 22,
                        fields: vec![],
                    });
                }
                let stack = Stack::new(1, extras).map_err(|error| bad(error.to_string()))?;
                result
                    .add_selected_stack(item, stack)
                    .map_err(|error| bad(error.to_string()))?;
            }
        }
        Ok(result)
    }

    fn outfit_wear_slots(&self, item: GlobalFormId) -> Result<Option<u32>, Error> {
        let record = self
            .record(item)
            .filter(|record| !record.is_deleted())
            .ok_or_else(|| bad("missing selected outfit item"))?;
        match record.tag.as_bytes() {
            b"ARMO" => {
                let armor = self
                    .combat_armor_record(item)
                    .map_err(|error| bad(error.to_string()))?;
                if armor.kind.is_some_and(|kind| kind > 2) {
                    return Err(bad("unknown outfit armor skill"));
                }
                Ok(Some(armor.slots))
            }
            // CommonLib TESObjectWEAP, TESAmmo and TESObjectLIGH declare these equip forms.
            b"WEAP" | b"AMMO" => Ok(Some(0)),
            b"LIGH" => Ok(record
                .find(T(b"DATA"))
                .and_then(|bytes| bytes.get(12..16))
                .and_then(le_u32)
                .filter(|flags| flags & 2 != 0)
                .map(|_| 0)),
            _ => Ok(None),
        }
    }
}

fn bad(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
