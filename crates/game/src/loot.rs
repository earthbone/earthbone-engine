// SPDX-License-Identifier: GPL-3.0-or-later

//! Exact loot transactions over the shared inventory. QuickLoot IE
//! Items/ItemStack.cpp supplies corpse and ownership semantics, while the
//! existing inventory notifier supplies CK OnItemAdded/OnItemRemoved events.

use soe_esm::{GlobalFormId, Tag};

use crate::inventory::{Error, Field, Selection};
use crate::world::World;
use crate::Game;

/// Whether a reference can supply a crosshair loot list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootState {
    /// An unlocked container or nonsummoned corpse.
    Ready,
    /// Inventory stays hidden until normal activation unlocks it.
    Locked,
    /// Living actors, blocked, disabled and deleted references are ineligible.
    Hidden,
}

impl World {
    /// Whether taking a loose reference violates its reference or cell ownership.
    pub fn loose_item_is_stealing(&mut self, reference: GlobalFormId, actor: GlobalFormId) -> bool {
        let rank = self
            .reference(reference)
            .and_then(|reference| reference.owner_rank);
        self.loot_owner(reference).is_some_and(|owner| {
            self.base_of(actor) != Some(owner)
                && !self.actor(actor).is_some_and(|actor| {
                    actor
                        .factions
                        .get(&owner)
                        .is_some_and(|member| *member >= rank.unwrap_or(0))
                })
        })
    }

    /// Read eligibility without activating or opening the reference.
    pub fn loot_state(&mut self, holder: GlobalFormId) -> LootState {
        let Some(reference) = self.reference(holder) else {
            return LootState::Hidden;
        };
        if !reference.enabled || reference.deleted || reference.activation_blocked {
            return LootState::Hidden;
        }
        let (base, actor, locked) = (reference.base, reference.is_actor, reference.locked);
        let valid = if actor {
            self.actor(holder).is_some_and(|actor| actor.dead)
                && !self
                    .active_effects()
                    .any(|effect| effect.summoned == Some(holder))
        } else {
            base.is_some_and(|base| self.tag_of(base) == Some(Tag(*b"CONT")))
        };
        if !valid {
            LootState::Hidden
        } else if locked {
            LootState::Locked
        } else {
            LootState::Ready
        }
    }

    /// Effective reference ownership includes its cell's owner.
    pub fn loot_owner(&mut self, holder: GlobalFormId) -> Option<GlobalFormId> {
        let reference = self.reference(holder)?;
        let (owner, cell) = (reference.owner, reference.cell);
        owner.or_else(|| cell.and_then(|cell| self.cell_state(cell).and_then(|cell| cell.owner)))
    }

    /// Evaluate the selected stack's ownership against the taking actor.
    /// Dead actors' contents are free to take, as in QuickLoot IE.
    ///
    /// # Errors
    /// Rejects stale or malformed inventory selections.
    pub fn loot_is_stealing(
        &mut self,
        holder: GlobalFormId,
        actor: GlobalFormId,
        selected: &Selection,
    ) -> Result<bool, Error> {
        let reference = self.reference(holder).ok_or(Error::Selection)?;
        let stack = reference.inventory.selected_stack(selected)?;
        let owner = stack.and_then(|stack| stack.extra(33)).and_then(|extra| {
            if let [Field::Form(owner)] = extra.fields.as_slice() {
                *owner
            } else {
                None
            }
        });
        let rank = stack.and_then(|stack| stack.extra(35)).and_then(|extra| {
            if let [Field::Word(rank)] = extra.fields.as_slice() {
                Some(*rank as i32)
            } else {
                None
            }
        });
        if holder == actor || self.actor_ref(holder).is_some_and(|actor| actor.dead) {
            return Ok(false);
        }
        let Some(container_owner) = self.loot_owner(holder) else {
            return Ok(false);
        };
        let owner = owner.unwrap_or(container_owner);
        let allowed = self.base_of(actor) == Some(owner)
            || self.actor(actor).is_some_and(|actor| {
                actor
                    .factions
                    .get(&owner)
                    .is_some_and(|member| *member >= rank.unwrap_or(0))
            });
        Ok(!allowed)
    }
}

impl Game {
    /// Take an exact stack selection into the player inventory and notify both
    /// holders. Overflow, stale selection or changed eligibility changes nothing.
    /// Existing extra lists carry stolen ownership through ordinary saves.
    ///
    /// # Errors
    /// Rejects ineligible references, stale selections and recipient overflow.
    pub fn loot_item(&mut self, holder: GlobalFormId, selected: &Selection) -> Result<i32, Error> {
        let player = self.player();
        let moved = {
            let mut world = self.world.borrow_mut();
            if holder == player || world.loot_state(holder) != LootState::Ready {
                return Err(Error::Selection);
            }
            let stealing = world.loot_is_stealing(holder, player, selected)?;
            let owner = if stealing {
                world.loot_owner(holder)
            } else {
                None
            };
            let mut source = world
                .reference(holder)
                .ok_or(Error::Selection)?
                .inventory
                .clone();
            let mut target = world
                .reference(player)
                .ok_or(Error::Selection)?
                .inventory
                .clone();
            let mut bundle = source.take_selected(selected)?;
            bundle.unequip();
            bundle.inherit_owner(owner);
            target.receive(selected.item(), bundle.clone())?;
            world.reference(holder).ok_or(Error::Selection)?.inventory = source;
            world.reference(player).ok_or(Error::Selection)?.inventory = target;
            let vm = &mut self.vm;
            world.notify_item_bundle(
                holder,
                selected.item(),
                Some(player),
                &bundle,
                &mut |name, class| vm.create_object_of(name, class),
            )
        };
        self.run_until_idle(64);
        Ok(moved)
    }
}
