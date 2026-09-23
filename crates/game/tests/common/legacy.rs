// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips both the exclusive dialogue branch version 24 added and the stagger
/// end time version 29 added, so fixtures built against either older header
/// see neither field's bytes.
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
        saved.push((*id, prior_branch, sentinel, prior_stagger, stagger_sentinel));
    }
    let mut bytes = world.save_state();
    for (id, prior_branch, _, prior_stagger, _) in &saved {
        let actor = world.actor(*id).unwrap();
        actor.exclusive_branch = *prior_branch;
        actor.stagger_end_time = *prior_stagger;
    }
    assert_eq!(
        world.state_hash(),
        hash,
        "legacy fixture changed world state"
    );
    let mut fields = Vec::new();
    for (_, _, sentinel, _, stagger_sentinel) in saved {
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
    }
    fields.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in fields {
        bytes.drain(offset..offset + len);
    }
    bytes
}
