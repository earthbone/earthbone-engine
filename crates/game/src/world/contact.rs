// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared placement of the actors a host contact query may strike, and the
//! reach of a weapon swing. The World owns both. A host presents the
//! placements as physics bodies and rebuilds them from this list whenever
//! the loaded cells change, so no host node is needed per reference.
//! Exterior eligibility uses CellIndex's UESP CELL/XCLC and WorldChildren
//! mapping. Persistent parent cells retain identity, while current position
//! determines whether the actor occupies the attached exterior grid.
//! A transient FormID index follows shared reference invalidation, avoiding
//! scenery scans while leaving placement and enable state in World.

use std::collections::BTreeSet;

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

#[derive(Default)]
pub(super) struct ContactReferences {
    initialized: bool,
    ids: BTreeSet<GlobalFormId>,
    dirty: BTreeSet<GlobalFormId>,
    #[cfg(test)]
    examined: usize,
}

impl ContactReferences {
    pub(super) fn invalidate(&mut self, id: GlobalFormId) {
        if self.initialized {
            self.dirty.insert(id);
        }
    }
}

impl World {
    pub(super) fn contact_reference_ids(&self) -> std::cell::Ref<'_, ContactReferences> {
        let mut index = self.contact_references.borrow_mut();
        if index.initialized {
            for id in std::mem::take(&mut index.dirty) {
                #[cfg(test)]
                {
                    index.examined += 1;
                }
                if self
                    .refs
                    .get(&id)
                    .is_some_and(|reference| reference.is_actor)
                {
                    index.ids.insert(id);
                } else {
                    index.ids.remove(&id);
                }
            }
        } else {
            #[cfg(test)]
            {
                index.examined += self.refs.len();
            }
            index.ids = self
                .refs
                .iter()
                .filter_map(|(&id, reference)| reference.is_actor.then_some(id))
                .collect();
            index.initialized = true;
            index.dirty.clear();
        }
        drop(index);
        self.contact_references.borrow()
    }

    /// Coordinate space shared by exterior grid and persistent cells. Interior
    /// cells retain their own identity. Uses the winning CellIndex hierarchy.
    #[must_use]
    pub fn cell_coordinate_space(&self, cell: GlobalFormId) -> GlobalFormId {
        let index = self.order.build_cell_index();
        index.grid_of(cell).map_or_else(
            || {
                self.record(cell)
                    .and_then(|record| record.placement.world)
                    .and_then(|raw| self.resolve_from(cell, raw))
                    .filter(|world| index.world_persistent_cell(*world) == Some(cell))
                    .unwrap_or(cell)
            },
            |grid| grid.world,
        )
    }

    /// Every actor reference that stands enabled in an attached cell,
    /// sorted by form. A cell is attached when a transition loaded it or the
    /// exterior ring supplied host resources, see [`World::cell_is_attached`].
    /// Exterior placement must also occupy an attached worldspace grid.
    /// Loading
    /// a cell builds its references, so an actor needs no run-time state yet
    /// to be listed. Dead actors stay listed because a corpse still blocks a
    /// blade.
    ///
    /// An actor whose controller scale cannot be derived keeps scale 1.0
    /// times its reference scale, so a template error never drops a body.
    pub fn contact_actors(&mut self) -> Vec<ContactActor> {
        let mut actors = self.contact_actor_placements();
        self.resolve_contact_actor_scales(&mut actors);
        actors
    }

    /// Enabled attached actor placements in FormID order, before resolving
    /// controller height. `scale` contains the reference scale only. Hosts
    /// can select detection participants before parsing their base records,
    /// then call [`Self::resolve_contact_actor_scales`] on the selected list.
    pub fn contact_actor_placements(&self) -> Vec<ContactActor> {
        let index = self.order.build_cell_index();
        let mut grids: std::collections::HashMap<GlobalFormId, std::collections::HashSet<_>> =
            std::collections::HashMap::new();
        for cell in self.attached_cell_ids() {
            if let Some(grid) = index.grid_of(cell) {
                grids
                    .entry(grid.world)
                    .or_default()
                    .insert((grid.x, grid.y));
            }
        }
        let mut cell_worlds = std::collections::HashMap::new();
        let actors = self.contact_reference_ids();
        let mut out = Vec::new();
        for &actor in &actors.ids {
            let Some(reference) = self.refs.get(&actor) else {
                continue;
            };
            let (Some(cell), Some(_)) = (reference.cell, reference.base) else {
                continue;
            };
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
            let world = *cell_worlds.entry(cell).or_insert_with(|| {
                let space = self.cell_coordinate_space(cell);
                (space != cell).then_some(space)
            });
            let attached = world.and_then(|world| grids.get(&world)).map_or_else(
                || self.loaded_cell == Some(cell) || self.cell_is_attached(cell),
                |cells| {
                    let x = (position[0] / 4096.0).floor() as i32;
                    let y = (position[1] / 4096.0).floor() as i32;
                    cells.contains(&(x, y))
                },
            );
            if !attached
                || reference.deleted
                || reference.destroyed
                || !self.reference_enabled(actor)
            {
                continue;
            }
            let placed = if placed.is_finite() && placed > 0.0 {
                placed
            } else {
                1.0
            };
            out.push(ContactActor {
                actor,
                cell,
                position,
                rotation,
                scale: placed,
            });
        }
        out
    }

    /// Resolve controller height for a selected placement list. Each base is
    /// read once in this call, using current winning records and templates.
    /// Invalid height retains the contact path's reference-scale fallback.
    pub fn resolve_contact_actor_scales(&self, actors: &mut [ContactActor]) {
        let mut heights = std::collections::HashMap::new();
        for actor in actors {
            let Some(reference) = self.refs.get(&actor.actor) else {
                continue;
            };
            let Some(base) = reference.base else { continue };
            let height = *heights
                .entry(base)
                .or_insert_with(|| self.actor_controller_scale(base).unwrap_or(1.0));
            let placed = reference.scale;
            let placed = if placed.is_finite() && placed > 0.0 {
                placed
            } else {
                1.0
            };
            let scale = height * placed;
            actor.scale = if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            };
        }
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

#[cfg(test)]
mod prf_31_tests {
    use super::*;
    use crate::world::RefState;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"CELL"), 1).sub(b"DATA", [1, 0]));
        plugin.push_top(Record::new(Tag(*b"NPC_"), 2));
        let actor = Record::new(Tag(*b"ACHR"), 3).sub(b"NAME", 2_u32.to_le_bytes());
        let mut actor = actor.sub(b"DATA", [0; 24]);
        actor.placement.cell = Some(1);
        plugin.push_top(actor);
        let mut order = LoadOrder::new();
        order.add("Contacts.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.load_cell(id(1));
        world
    }

    #[test]
    fn prf_31_contact_index_bounds_unchanged_frames_independently_of_scenery() {
        let mut world = world();
        for object in 10..5010 {
            world.refs.insert(id(object), RefState::new(None));
        }
        *world.contact_references.get_mut() = ContactReferences::default();
        let initial_work = world.contact_reference_ids().examined;
        assert_eq!(initial_work, world.refs.len());
        for _ in 0..60 {
            assert_eq!(world.contact_actor_placements().len(), 1);
        }
        assert_eq!(world.contact_reference_ids().examined, initial_work);
        world.reference(id(3)).unwrap().position = [50.0, 0.0, 0.0];
        assert_eq!(
            world.contact_actor_placements()[0].position,
            [50.0, 0.0, 0.0]
        );
        assert_eq!(world.contact_reference_ids().examined, initial_work + 1);
    }

    #[test]
    fn prf_31_contact_index_follows_actor_flag_changes_and_removal() {
        let mut world = world();
        assert_eq!(world.contact_actor_placements().len(), 1);
        world.reference(id(3)).unwrap().is_actor = false;
        assert!(world.contact_actor_placements().is_empty());
        world.reference(id(3)).unwrap().is_actor = true;
        assert_eq!(world.contact_actor_placements().len(), 1);
        world.invalidate_reference(id(3));
        world.refs.remove(&id(3));
        assert!(world.contact_actor_placements().is_empty());
        world.reference(id(3)).unwrap();
        assert_eq!(world.contact_actor_placements().len(), 1);
    }
}
