// SPDX-License-Identifier: GPL-3.0-or-later

//! COC arrival precedence follows OpenMW worldimp.cpp findInteriorPosition and
//! findExteriorPosition, CK CenterOnCell and UESP Skyrim Console. Skyrim XTEL
//! destinations belong to the peer door, not the door being entered from.

use crate::World;
use soe_esm::{records::refr::Refr, records::Transform, CellIndex, GlobalFormId, Tag};

fn finite(transform: Transform) -> bool {
    transform
        .position
        .into_iter()
        .chain(transform.rotation)
        .all(f32::is_finite)
}

pub(super) fn destination(
    world: &World,
    index: &CellIndex,
    cell: GlobalFormId,
) -> Result<Transform, String> {
    let order = world.order();
    let grid = index.grid_of(cell);
    let mut ids = index.references(cell).to_vec();
    if let Some(persistent) = grid.and_then(|grid| index.world_persistent_cell(grid.world)) {
        ids.extend_from_slice(index.references(persistent));
    }
    ids.sort_unstable();
    ids.dedup();
    let mut candidates = Vec::new();
    for id in ids {
        if !world.reference_enabled(id) {
            continue;
        }
        let Some(owner) = order.winner_ref(id) else {
            continue;
        };
        let record = order.record(owner);
        if record.tag != Tag(*b"REFR") {
            continue;
        }
        let Ok(reference) = Refr::from_record(record) else {
            continue;
        };
        let Some(mut transform) = reference.transform else {
            continue;
        };
        if let Some(state) = world.reference_ref(id) {
            if state.cell != Some(cell) && index.references(cell).contains(&id) {
                continue;
            }
            transform.position = state.position;
            transform.rotation = state.rotation;
        }
        if !finite(transform)
            || grid.is_some_and(|grid| {
                (transform.position[0] / 4096.0).floor() as i32 != grid.x
                    || (transform.position[1] / 4096.0).floor() as i32 != grid.y
            })
        {
            continue;
        }
        let Some(base) = order.winner(order.resolve(owner.file, reference.base)) else {
            continue;
        };
        let name = base.editor_id().unwrap_or_default();
        let rank = if name.eq_ignore_ascii_case("COCMarkerHeading") {
            0
        } else if name.eq_ignore_ascii_case("XMarkerHeading") {
            2
        } else if [Tag(*b"CONT"), Tag(*b"FURN"), Tag(*b"ACTI")].contains(&base.tag) {
            3
        } else if base.tag == Tag(*b"STAT") && !name.to_ascii_lowercase().contains("marker") {
            4
        } else {
            continue;
        };
        candidates.push((rank, id, transform));
    }
    for &id in index.references(cell) {
        if !world.reference_enabled(id) {
            continue;
        }
        let Some(owner) = order.winner_ref(id) else {
            continue;
        };
        let record = order.record(owner);
        if record.tag != Tag(*b"REFR") {
            continue;
        }
        let Ok(reference) = Refr::from_record(record) else {
            continue;
        };
        let Some(link) = reference.teleport else {
            continue;
        };
        let peer_id = order.resolve(owner.file, link.door);
        let Some(peer_owner) = order.winner_ref(peer_id) else {
            continue;
        };
        let peer = order.record(peer_owner);
        if peer.tag != Tag(*b"REFR") || !world.reference_enabled(peer_id) {
            continue;
        }
        let Ok(peer) = Refr::from_record(peer) else {
            continue;
        };
        let Some(arrival) = peer.teleport else {
            continue;
        };
        if order.resolve(peer_owner.file, arrival.door) == id && finite(arrival.destination) {
            candidates.push((1, id, arrival.destination));
        }
    }
    candidates.sort_by_key(|(rank, id, _)| (*rank, *id));
    if let Some((_, _, transform)) = candidates.first() {
        return Ok(*transform);
    }
    if let Some(grid) = grid {
        let height = index
            .land(cell)
            .and_then(|id| order.winner(id))
            .and_then(|record| soe_esm::records::land::Land::from_record(record).ok())
            .and_then(|land| land.heights)
            .map_or(0.0, |heights| heights.decode()[16 * 33 + 16]);
        return Ok(Transform {
            position: [
                (grid.x as f32 + 0.5) * 4096.0,
                (grid.y as f32 + 0.5) * 4096.0,
                height,
            ],
            rotation: [0.0; 3],
        });
    }
    Err("cell has no suitable arrival reference".into())
}
