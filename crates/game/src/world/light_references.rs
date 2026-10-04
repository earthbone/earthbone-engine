// SPDX-License-Identifier: GPL-3.0-or-later

//! Derived light-reference membership for host detection. xEdit's REFR/NAME
//! links a placement to LIGH. Only identities are cached, all mutable light
//! state remains in World. Cell membership uses the current reference cell.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;

use super::{World, T};

#[derive(Default)]
pub(super) struct LightReferences {
    authored: BTreeMap<GlobalFormId, Vec<GlobalFormId>>,
    loaded: Option<BTreeSet<GlobalFormId>>,
}

impl World {
    /// Light references currently belonging to `cell`, including runtime
    /// spawns and moved authored references, in FormID order. Disabled and
    /// deleted references remain members so enabling them needs no rebuild.
    /// Winning authored membership is built once per cell. Loaded membership
    /// is derived once and extended as references are instantiated.
    pub fn light_references_in_cell(&self, cell: GlobalFormId) -> Vec<GlobalFormId> {
        let mut index = self.light_references.borrow_mut();
        let loaded = index.loaded.get_or_insert_with(|| {
            self.refs
                .keys()
                .copied()
                .filter(|id| self.reference_is_light(*id))
                .collect()
        });
        let mut result: BTreeSet<_> = loaded
            .iter()
            .copied()
            .filter(|id| {
                self.refs
                    .get(id)
                    .is_some_and(|reference| reference.cell == Some(cell))
            })
            .collect();
        let authored = index.authored.entry(cell).or_insert_with(|| {
            self.order
                .build_cell_index()
                .references(cell)
                .iter()
                .copied()
                .filter(|id| self.reference_is_light(*id))
                .collect()
        });
        result.extend(authored.iter().copied().filter(|id| {
            self.refs
                .get(id)
                .is_none_or(|reference| reference.cell == Some(cell))
        }));
        result.into_iter().collect()
    }

    pub(super) fn track_light_reference(&self, id: GlobalFormId) {
        if !self.reference_is_light(id) {
            return;
        }
        if let Some(loaded) = &mut self.light_references.borrow_mut().loaded {
            loaded.insert(id);
        }
    }

    fn reference_is_light(&self, id: GlobalFormId) -> bool {
        let base = if let Some(reference) = self.refs.get(&id) {
            reference.base
        } else {
            self.record(id)
                .filter(|record| record.tag == T(b"REFR"))
                .and_then(|record| record.find(T(b"NAME")))
                .filter(|bytes| bytes.len() == 4)
                .and_then(|bytes| {
                    self.resolve_from(
                        id,
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                    )
                })
        };
        base.is_some_and(|base| self.tag_of(base) == Some(T(b"LIGH")))
    }
}

#[cfg(test)]
mod prf_31_tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(T(b"CELL"), 1));
        plugin.push_top(Record::new(T(b"CELL"), 2));
        plugin.push_top(Record::new(T(b"LIGH"), 3));
        plugin.push_top(Record::new(T(b"STAT"), 4));
        for (object, base) in [(5_u32, 3_u32), (6, 4)] {
            let mut reference = Record::new(T(b"REFR"), object).sub(b"NAME", base.to_le_bytes());
            reference.placement.cell = Some(1);
            plugin.push_top(reference);
        }
        let mut order = LoadOrder::new();
        order.add("Lights.esm", plugin).unwrap();
        World::new(order)
    }

    #[test]
    fn prf_31_light_membership_follows_moves_spawns_and_visibility() {
        let mut world = world();
        assert_eq!(world.light_references_in_cell(id(1)), [id(5)]);
        assert!(world.light_references_in_cell(id(2)).is_empty());
        world.reference(id(6)).unwrap().cell = Some(id(2));
        world.move_to(id(5), id(6), [0.0; 3]);
        assert!(world.light_references_in_cell(id(1)).is_empty());
        assert_eq!(world.light_references_in_cell(id(2)), [id(5)]);
        let spawned = world.place_at(id(3), id(5), true);
        assert_eq!(world.light_references_in_cell(id(2)), [id(5), spawned]);
        world.reference(spawned).unwrap().enabled = true;
        world.reference(id(5)).unwrap().deleted = true;
        assert_eq!(world.light_references_in_cell(id(2)), [id(5), spawned]);
        assert!(world.move_to_editor_location(id(5)));
        assert_eq!(world.light_references_in_cell(id(1)), [id(5)]);
    }

    #[test]
    fn prf_31_native_restore_rebuilds_light_membership() {
        let mut world = world();
        world.load_cell(id(1));
        let spawned = world.place_at(id(3), id(5), false);
        let saved = world.save_state();
        world.reference(id(6)).unwrap().cell = Some(id(2));
        world.move_to(spawned, id(6), [0.0; 3]);
        assert_eq!(world.light_references_in_cell(id(2)), [spawned]);
        world.load_state(&saved).unwrap();
        assert_eq!(world.light_references_in_cell(id(1)), [id(5), spawned]);
        assert!(world.light_references_in_cell(id(2)).is_empty());
    }
}
