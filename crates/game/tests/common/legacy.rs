// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips the exclusive dialogue branch version 24 added, the stagger end time
/// version 29 added and the witnessed crime set version 31 added, so fixtures
/// built against any older header see none of those fields' bytes.
pub fn without_exclusive_branches(world: &mut World, actors: &[GlobalFormId]) -> Vec<u8> {
    let hash = world.state_hash();
    let mut saved = Vec::new();
    for (index, id) in actors.iter().enumerate() {
        assert!(
            world.actor_ref(*id).is_some(),
            "legacy fixture actor is initialized"
        );
        let actor = world.actor(*id).unwrap();
        let prior_branch = actor.exclusive_branch;
        let sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xF000_0000 + index as u32,
        };
        actor.exclusive_branch = Some(sentinel);
        let prior_stagger = actor.stagger_end_time;
        let stagger_sentinel = 1_000_000.0 + f64::from(index as u32);
        actor.stagger_end_time = Some(stagger_sentinel);
        let witness_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xE000_0000 + index as u32,
        };
        let prior_witnessed =
            std::mem::replace(&mut actor.witnessed_crimes, [witness_sentinel].into());
        saved.push(Saved {
            id: *id,
            prior_branch,
            sentinel,
            prior_stagger,
            stagger_sentinel,
            prior_witnessed,
            witness_sentinel,
        });
    }
    let mut bytes = world.save_state();
    for entry in &mut saved {
        let actor = world.actor(entry.id).unwrap();
        actor.exclusive_branch = entry.prior_branch;
        actor.stagger_end_time = entry.prior_stagger;
        actor.witnessed_crimes = std::mem::take(&mut entry.prior_witnessed);
    }
    assert_eq!(
        world.state_hash(),
        hash,
        "legacy fixture changed world state"
    );
    let mut fields = Vec::new();
    for Saved {
        sentinel,
        stagger_sentinel,
        witness_sentinel,
        ..
    } in saved
    {
        let mut marker = vec![1];
        marker.extend_from_slice(&sentinel.file.to_le_bytes());
        marker.extend_from_slice(&sentinel.object.to_le_bytes());
        let offsets: Vec<_> = bytes
            .windows(marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == marker).then_some(offset))
            .collect();
        assert_eq!(offsets.len(), 1, "exclusive sentinel is not unique");
        fields.push((offsets[0], marker.len()));

        let mut stagger_marker = vec![1];
        stagger_marker.extend_from_slice(&stagger_sentinel.to_le_bytes());
        let stagger_offsets: Vec<_> = bytes
            .windows(stagger_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == stagger_marker).then_some(offset))
            .collect();
        assert_eq!(stagger_offsets.len(), 1, "stagger sentinel is not unique");
        fields.push((stagger_offsets[0], stagger_marker.len()));

        let mut witness_marker = 1u32.to_le_bytes().to_vec();
        witness_marker.extend_from_slice(&witness_sentinel.file.to_le_bytes());
        witness_marker.extend_from_slice(&witness_sentinel.object.to_le_bytes());
        let witness_offsets: Vec<_> = bytes
            .windows(witness_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == witness_marker).then_some(offset))
            .collect();
        assert_eq!(witness_offsets.len(), 1, "witness sentinel is not unique");
        fields.push((witness_offsets[0], witness_marker.len()));
    }
    fields.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in fields {
        bytes.drain(offset..offset + len);
    }
    bytes
}

/// One actor's replaced fields and the sentinels written in their place.
struct Saved {
    id: GlobalFormId,
    prior_branch: Option<GlobalFormId>,
    sentinel: GlobalFormId,
    prior_stagger: Option<f64>,
    stagger_sentinel: f64,
    prior_witnessed: std::collections::BTreeSet<GlobalFormId>,
    witness_sentinel: GlobalFormId,
}
