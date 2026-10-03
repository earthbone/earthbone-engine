// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib extra-data views over the shared reference and inventory state.
//!
//! Value identities follow the local CommonLibSSE-NG `include/RE/E` headers.

use super::{le_f32, le_u32, text, World, T};
use crate::inventory::Field;
use soe_commonlib_shim::extra_data::{
    DoorTeleportData, ReferenceExtra, ReferenceExtraData, ReferenceExtraKind,
};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::ni::NiPoint3;
use soe_commonlib_shim::refr::{
    ExtraLeveledItemBase, ExtraLightData, ExtraMapMarker, LinkedRefChild, MissingLinkedRefId,
    ObjectRefHandle, ReferenceExtraData as ReferenceExtraView,
};
use soe_esm::records::refr::Refr;
use soe_esm::GlobalFormId;

/// Borrowed reference view. Derived link arrays cannot outlive the world borrow.
pub struct ReferenceExtrasView<'a> {
    world: &'a World,
    reference: GlobalFormId,
    children: Vec<LinkedRefChild>,
    missing: Vec<MissingLinkedRefId>,
}

impl World {
    /// Read reference extras from current records and loaded reference state.
    /// Returns no view for a reference that is not loaded.
    pub fn shim_reference_extras(
        &self,
        reference: GlobalFormId,
    ) -> Option<ReferenceExtrasView<'_>> {
        let state = self.refs.get(&reference)?;
        let mut children = Vec::new();
        for (child, state) in &self.refs {
            for (keyword, target) in &state.linked {
                if *target == reference {
                    children.push(LinkedRefChild {
                        keyword: FormId(keyword.map_or(0, |id| self.runtime_id(id))),
                        reference: ObjectRefHandle(self.runtime_id(*child)),
                    });
                }
            }
        }
        for (child, record) in self.order.winners_of(T(b"REFR")) {
            for bytes in record.all(T(b"XLKR")) {
                let (keyword, raw_target) = match bytes.len() {
                    4 => (None, le_u32(bytes)?),
                    8 => (Some(le_u32(bytes)?), le_u32(&bytes[4..])?),
                    _ => continue,
                };
                if self.resolve_from(child, raw_target) == Some(reference) {
                    let keyword = keyword
                        .and_then(|raw| self.resolve_from(child, raw))
                        .map(|id| FormId(self.runtime_id(id)))
                        .unwrap_or(FormId(0));
                    children.push(LinkedRefChild {
                        keyword,
                        reference: ObjectRefHandle(self.runtime_id(child)),
                    });
                }
            }
        }
        children.sort_by_key(|entry| (entry.reference.0, entry.keyword.0));
        children.dedup_by_key(|entry| (entry.reference.0, entry.keyword.0));
        let mut missing: Vec<_> = state
            .linked
            .iter()
            .filter(|(_, target)| !self.exists(*target))
            .map(|(keyword, target)| MissingLinkedRefId {
                keyword: FormId(keyword.map_or(0, |id| self.runtime_id(id))),
                linked_reference: FormId(self.runtime_id(*target)),
            })
            .collect();
        missing.sort_by_key(|entry| (entry.keyword.0, entry.linked_reference.0));
        Some(ReferenceExtrasView {
            world: self,
            reference,
            children,
            missing,
        })
    }
}

impl ReferenceExtraView for ReferenceExtrasView<'_> {
    fn leveled_item_base(&self) -> Option<ExtraLeveledItemBase> {
        let record = self.world.record(self.reference)?;
        let raw = le_u32(record.find(T(b"XLIB"))?)?;
        let id = self.world.resolve_from(self.reference, raw)?;
        if self.world.tag_of(id)? != T(b"LVLI") {
            return None;
        }
        Some(ExtraLeveledItemBase {
            leveled_item: FormId(self.world.runtime_id(id)),
        })
    }

    fn light_data(&self) -> Option<ExtraLightData> {
        let bytes = self.world.record(self.reference)?.find(T(b"XLIG"))?;
        if !matches!(bytes.len(), 16 | 20) {
            return None;
        }
        Some(ExtraLightData {
            fov: le_f32(bytes),
            fade: le_f32(&bytes[4..]),
            end_distance_cap: le_f32(&bytes[8..]),
            shadow_depth_bias: le_f32(&bytes[12..]),
            unknown: bytes.get(16).copied().unwrap_or(0),
        })
    }

    fn linked_ref_children(&self) -> &[LinkedRefChild] {
        &self.children
    }

    fn map_marker(&self) -> Option<ExtraMapMarker> {
        let record = self.world.record(self.reference)?;
        record.find(T(b"XMRK"))?;
        let state = self.world.refs.get(&self.reference)?;
        let flags = *record.find(T(b"FNAM"))?.first()?;
        let marker_type = u16::from(*record.find(T(b"TNAM"))?.first()?);
        let name = if let Some(name) = &state.display_name {
            name.clone()
        } else {
            let name = text(
                record.find(T(b"FULL"))?,
                self.world.record_text_localized(self.reference, T(b"FULL")),
            );
            self.world.resolve_text(self.reference, T(b"FULL"), &name)
        };
        Some(ExtraMapMarker {
            name,
            flags: flags | u8::from(state.on_map) | (u8::from(state.fast_travel) << 1),
            marker_type,
        })
    }

    fn missing_linked_ref_ids(&self) -> &[MissingLinkedRefId] {
        &self.missing
    }
}

impl ReferenceExtraData for World {
    fn extra(
        &self,
        reference: ObjectRefHandle,
        kind: ReferenceExtraKind,
    ) -> Option<ReferenceExtra> {
        let id = self.from_runtime_id(reference.0)?;
        match kind {
            ReferenceExtraKind::MissingRefIds => {
                let record = self.record(id)?;
                let mut parents = Vec::new();
                for bytes in record.all(T(b"XAPR")) {
                    if bytes.len() != 8 {
                        return None;
                    }
                    let raw = le_u32(bytes)?;
                    let activate_delay = le_f32(&bytes[4..]);
                    let parent = self.resolve_from(id, raw)?;
                    if !matches!(self.tag_of(parent), Some(tag) if tag == T(b"REFR") || tag == T(b"ACHR"))
                    {
                        return None;
                    }
                    if !activate_delay.is_finite() {
                        return None;
                    }
                    parents.push(soe_commonlib_shim::extra_data::ActivateParentId {
                        reference: form_id(self.runtime_id(parent)),
                        activate_delay,
                    });
                }
                (!parents.is_empty()).then_some(ReferenceExtra::MissingRefIds(parents))
            }
            ReferenceExtraKind::NorthRotation => {
                let record = self.record(id)?;
                if record.tag != T(b"CELL") {
                    return None;
                }
                let flags = record.find(T(b"DATA"))?;
                if flags.len() < 2 || u16::from_le_bytes([flags[0], flags[1]]) & 1 == 0 {
                    return None;
                }
                Some(ReferenceExtra::NorthRotation(0.0))
            }
            ReferenceExtraKind::Teleport => {
                let state = self.refs.get(&id)?;
                let (linked_door, destination) = state.teleport?;
                let flags = self
                    .record(id)
                    .and_then(|record| Refr::from_record(record).ok())
                    .and_then(|reference| reference.teleport)
                    .and_then(|teleport| teleport.flags)
                    .unwrap_or_default() as i8;
                Some(ReferenceExtra::Teleport(DoorTeleportData {
                    linked_door: ObjectRefHandle(self.runtime_id(linked_door)),
                    position: point(destination.position),
                    rotation: point(destination.rotation),
                    flags,
                }))
            }
            ReferenceExtraKind::OutfitItem => {
                let state = self.refs.get(&id)?;
                state.inventory.iter().find_map(|(item, _)| {
                    state.inventory.stacks(*item).iter().find_map(|stack| {
                        let extra = stack.extra(142)?;
                        match extra.fields.as_slice() {
                            [Field::Form(Some(outfit))] => Some(ReferenceExtra::OutfitItem(
                                form_id(self.runtime_id(*outfit)),
                            )),
                            _ => None,
                        }
                    })
                })
            }
            ReferenceExtraKind::Poison => {
                let state = self.refs.get(&id)?;
                state.inventory.iter().find_map(|(item, _)| {
                    state.inventory.stacks(*item).iter().find_map(|stack| {
                        let extra = stack.extra(62)?;
                        match extra.fields.as_slice() {
                            [Field::Form(Some(poison)), Field::Word(count)] => {
                                Some(ReferenceExtra::Poison {
                                    poison: form_id(self.runtime_id(*poison)),
                                    count: *count,
                                })
                            }
                            _ => None,
                        }
                    })
                })
            }
        }
    }

    fn supports_extra(&self, kind: ReferenceExtraKind) -> bool {
        matches!(
            kind,
            ReferenceExtraKind::MissingRefIds
                | ReferenceExtraKind::NorthRotation
                | ReferenceExtraKind::OutfitItem
                | ReferenceExtraKind::Poison
                | ReferenceExtraKind::Teleport
        )
    }
}

fn form_id(value: u32) -> FormId {
    FormId(value)
}

fn point(value: [f32; 3]) -> NiPoint3 {
    NiPoint3 {
        x: value[0],
        y: value[1],
        z: value[2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Extra, Inventory, Stack};
    use crate::world::{RefState, DYNAMIC_FILE};
    use soe_commonlib_shim::extra_data::{reject_hook, ActivateParentId};
    use soe_esm::plugin::{FileHeader, Plugin, Record};
    use soe_esm::records::Transform;
    use soe_esm::{GlobalFormId, LoadOrder};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId {
            file: DYNAMIC_FILE,
            object,
        }
    }

    fn plugin_id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world_with_reference() -> (World, GlobalFormId) {
        let mut world = World::new(LoadOrder::default());
        let reference = id(1);
        let mut state = RefState::new(None);
        state.teleport = Some((
            id(2),
            Transform {
                position: [10.0, 20.0, 30.0],
                rotation: [0.1, 0.2, 0.3],
            },
        ));
        let item = id(3);
        let inventory = Inventory::default();
        let stack = Stack::new(
            1,
            vec![
                Extra {
                    kind: 142,
                    fields: vec![Field::Form(Some(id(4)))],
                },
                Extra {
                    kind: 62,
                    fields: vec![Field::Form(Some(id(5))), Field::Word(2)],
                },
            ],
        )
        .expect("fixture extras are valid");
        let mut inventory = inventory;
        inventory
            .restore(item, 1, vec![stack])
            .expect("fixture inventory is valid");
        state.inventory = inventory;
        world.refs.insert(reference, state);
        (world, reference)
    }

    #[test]
    fn shm_10_teleport_reads_live_reference_destination() {
        let (world, reference) = world_with_reference();
        let result = world.extra(
            ObjectRefHandle(world.runtime_id(reference)),
            ReferenceExtraKind::Teleport,
        );
        assert_eq!(
            result,
            Some(ReferenceExtra::Teleport(DoorTeleportData {
                linked_door: ObjectRefHandle(world.runtime_id(id(2))),
                position: point([10.0, 20.0, 30.0]),
                rotation: point([0.1, 0.2, 0.3]),
                flags: 0,
            }))
        );
    }

    #[test]
    fn shm_10_poison_reads_live_inventory_extra() {
        let (world, reference) = world_with_reference();
        assert_eq!(
            world.extra(
                ObjectRefHandle(world.runtime_id(reference)),
                ReferenceExtraKind::Poison,
            ),
            Some(ReferenceExtra::Poison {
                poison: FormId(world.runtime_id(id(5))),
                count: 2,
            })
        );
    }

    #[test]
    fn shm_10_outfit_item_reads_live_inventory_extra() {
        let (world, reference) = world_with_reference();
        assert_eq!(
            world.extra(
                ObjectRefHandle(world.runtime_id(reference)),
                ReferenceExtraKind::OutfitItem,
            ),
            Some(ReferenceExtra::OutfitItem(FormId(world.runtime_id(id(4)))))
        );
    }

    #[test]
    fn shm_10_supported_extras_and_binary_hooks_are_explicit() {
        let (world, _reference) = world_with_reference();
        assert!(world.supports_extra(ReferenceExtraKind::MissingRefIds));
        assert!(world.supports_extra(ReferenceExtraKind::NorthRotation));
        assert_eq!(
            reject_hook("write_vfunc"),
            Err(soe_commonlib_shim::extra_data::UnsupportedHook {
                name: "write_vfunc"
            })
        );
    }

    #[test]
    fn shm_10_missing_ref_ids_reads_resolved_live_activation_parents() {
        let mut plugin = Plugin::new(FileHeader::default());
        let mut xapr = 2u32.to_le_bytes().to_vec();
        xapr.extend_from_slice(&0.25f32.to_le_bytes());
        plugin.push_top(Record::new(T(b"REFR"), 1).sub(T(b"XAPR"), xapr));
        plugin.push_top(Record::new(T(b"REFR"), 2));
        let mut order = LoadOrder::new();
        order
            .add("SHM10.esm", plugin)
            .expect("fixture plugin is valid");
        let world = World::new(order);
        let result = world.extra(
            ObjectRefHandle(world.runtime_id(plugin_id(1))),
            ReferenceExtraKind::MissingRefIds,
        );
        assert_eq!(
            result,
            Some(ReferenceExtra::MissingRefIds(vec![ActivateParentId {
                reference: FormId(world.runtime_id(plugin_id(2))),
                activate_delay: 0.25,
            }]))
        );
    }

    #[test]
    fn shm_10_north_rotation_reads_interior_cell_default() {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(T(b"CELL"), 1).sub(T(b"DATA"), 1u16.to_le_bytes().to_vec()));
        let mut order = LoadOrder::new();
        order
            .add("SHM10.esm", plugin)
            .expect("fixture plugin is valid");
        let world = World::new(order);
        assert_eq!(
            world.extra(
                ObjectRefHandle(world.runtime_id(plugin_id(1))),
                ReferenceExtraKind::NorthRotation,
            ),
            Some(ReferenceExtra::NorthRotation(0.0))
        );
    }
}
