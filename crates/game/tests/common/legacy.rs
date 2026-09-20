// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
pub fn without_exclusive_branches(world: &mut World, actors: &[GlobalFormId]) -> Vec<u8> {
    let hash = world.state_hash();
    let mut saved = Vec::new();
    for (index, id) in actors.iter().enumerate() {
        assert!(
            world.actor_ref(*id).is_some(),
            "legacy fixture actor is initialized"
        );
        let actor = world.actor(*id).unwrap();
        let prior = actor.exclusive_branch;
        let sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xF000_0000 + index as u32,
        };
        actor.exclusive_branch = Some(sentinel);
        saved.push((*id, prior, sentinel));
    }
    let mut bytes = world.save_state();
    for (id, prior, _) in &saved {
        world.actor(*id).unwrap().exclusive_branch = *prior;
    }
    assert_eq!(
        world.state_hash(),
        hash,
        "legacy fixture changed world state"
    );
    let mut fields = Vec::new();
    for (_, _, sentinel) in saved {
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
    }
    fields.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in fields {
        bytes.drain(offset..offset + len);
    }
    bytes
}
