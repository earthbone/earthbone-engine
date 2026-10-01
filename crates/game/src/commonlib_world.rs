// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib world-state adapters. The MIT shim owns the contract types.

use crate::inventory::{Extra, Field};
use crate::world::World;
use soe_commonlib_shim::world::{self, ContainerItemExtra, WorldBindings};

impl world::BGSSaveLoadGame for World {
    fn save_state(&self) -> Vec<u8> {
        World::save_state(self)
    }

    fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        World::load_state(self, bytes).map_err(|error| error.to_string())
    }
}

impl WorldBindings for World {
    fn calculate_current_form_list(
        &mut self,
        list: u32,
        level: u16,
        count: i16,
    ) -> Result<Vec<world::CALCED_OBJECT>, String> {
        let list = self
            .from_runtime_id(list)
            .ok_or_else(|| format!("unresolved leveled list FormID {list:08X}"))?;
        let items = self
            .roll_leveled_inventory(list, u32::from(level), i32::from(count))
            .map_err(|error| error.to_string())?;
        items
            .iter()
            .flat_map(|(form, _)| items.stacks(*form).iter().map(move |stack| (*form, stack)))
            .map(|(form, stack)| {
                let count = u16::try_from(stack.count())
                    .map_err(|_| format!("calculated item count exceeds u16 for {form}"))?;
                let container_item = container_item_extra(self, stack.extras())?;
                Ok(world::CALCED_OBJECT {
                    form_id: self.runtime_id(form),
                    count,
                    container_item,
                })
            })
            .collect()
    }
}

fn container_item_extra(world: &World, extras: &[Extra]) -> Result<ContainerItemExtra, String> {
    let mut result = ContainerItemExtra::default();
    for extra in extras {
        let value = match (extra.kind, extra.fields.as_slice()) {
            (33 | 34, [Field::Form(Some(form))]) => Some(world.runtime_id(*form)),
            (35, [Field::Word(rank)]) | (37, [Field::FloatBits(rank)]) => Some(*rank),
            _ => {
                return Err(format!(
                    "unsupported calculated item extra-data type {}",
                    extra.kind
                ))
            }
        };
        match extra.kind {
            33 => result.owner = value,
            34 => result.owner_global = value,
            35 => result.owner_rank = value,
            37 => result.health_bits = value,
            _ => {
                return Err(format!(
                    "unsupported calculated item extra-data type {}",
                    extra.kind
                ))
            }
        }
    }
    Ok(result)
}
