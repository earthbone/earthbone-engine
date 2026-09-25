// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips the exclusive dialogue branch version 24 added, the stagger end time
/// version 29 added, the witnessed crime set version 31 added and the hand
/// spell fields version 32 added, so fixtures built against any older header
/// see none of those fields' bytes.
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
        let prior_left_spell = actor.left_spell;
        let left_spell_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xD000_0000 + index as u32,
        };
        actor.left_spell = Some(left_spell_sentinel);
        let prior_right_spell = actor.right_spell;
        let right_spell_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xC000_0000 + index as u32,
        };
        actor.right_spell = Some(right_spell_sentinel);
        saved.push(Saved {
            id: *id,
            prior_branch,
            sentinel,
            prior_stagger,
            stagger_sentinel,
            prior_witnessed,
            witness_sentinel,
            prior_left_spell,
            left_spell_sentinel,
            prior_right_spell,
            right_spell_sentinel,
        });
    }
    let mut bytes = world.save_state();
    for entry in &mut saved {
        let actor = world.actor(entry.id).unwrap();
        actor.exclusive_branch = entry.prior_branch;
        actor.stagger_end_time = entry.prior_stagger;
        actor.witnessed_crimes = std::mem::take(&mut entry.prior_witnessed);
        actor.left_spell = entry.prior_left_spell;
        actor.right_spell = entry.prior_right_spell;
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
        left_spell_sentinel,
        right_spell_sentinel,
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

        let mut left_spell_marker = vec![1];
        left_spell_marker.extend_from_slice(&left_spell_sentinel.file.to_le_bytes());
        left_spell_marker.extend_from_slice(&left_spell_sentinel.object.to_le_bytes());
        let left_spell_offsets: Vec<_> = bytes
            .windows(left_spell_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == left_spell_marker).then_some(offset))
            .collect();
        assert_eq!(
            left_spell_offsets.len(),
            1,
            "left spell sentinel is not unique"
        );
        fields.push((left_spell_offsets[0], left_spell_marker.len()));

        let mut right_spell_marker = vec![1];
        right_spell_marker.extend_from_slice(&right_spell_sentinel.file.to_le_bytes());
        right_spell_marker.extend_from_slice(&right_spell_sentinel.object.to_le_bytes());
        let right_spell_offsets: Vec<_> = bytes
            .windows(right_spell_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == right_spell_marker).then_some(offset))
            .collect();
        assert_eq!(
            right_spell_offsets.len(),
            1,
            "right spell sentinel is not unique"
        );
        fields.push((right_spell_offsets[0], right_spell_marker.len()));
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
    prior_left_spell: Option<GlobalFormId>,
    left_spell_sentinel: GlobalFormId,
    prior_right_spell: Option<GlobalFormId>,
    right_spell_sentinel: GlobalFormId,
}
