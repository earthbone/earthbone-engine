// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips the exclusive dialogue branch version 24 added, the stagger end time
/// version 29 added, the witnessed crime set version 31 added and the hand
/// spell fields version 32 added, so fixtures built against any older header
/// see none of those fields' bytes. Also strips the empty save-section table
/// version 37 appends to the actor's own record and, since `World::actor`
/// requires a paired reference (`self.reference(id)` with `is_actor` set), to
/// that reference's record too, so both records end exactly where a
/// pre-version-37 writer would have left them.
///
/// This file is pulled into many test binaries through `#[path]`, and each
/// binary compiles its own copy. Only some of them call this particular
/// helper, so the others see it as unused under `-D warnings`.
#[allow(dead_code)]
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

        // Every actor built through the public API has a paired reference
        // (`World::actor` requires `self.reference(id)` to already report
        // `is_actor`), and that reference's own record also ends with a
        // version 37 save-section table. `alpha` is the reference's last
        // field before that table, so a unique value here locates it the
        // same way the actor sentinels above locate the actor's own table.
        let reference = world.reference(*id).unwrap();
        let prior_alpha = reference.alpha;
        let alpha_sentinel = 700_000.0 + index as f32;
        reference.alpha = alpha_sentinel;

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
            prior_alpha,
            alpha_sentinel,
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
        world.reference(entry.id).unwrap().alpha = entry.prior_alpha;
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
        prior_alpha,
        alpha_sentinel,
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

        // The right spell field is immediately followed by this actor's
        // own empty save-section table (a `u16` count of 0, see
        // `save_sections`), which a pre-version-37 writer never emitted.
        fields.push((right_spell_offsets[0] + right_spell_marker.len(), 2));

        // `alpha` itself is not version gated, so unlike the fields above
        // it must stay in the stream with its true value. Only the empty
        // save-section table that follows it is a version 37 addition and
        // gets stripped. The marker bytes are overwritten back to
        // `prior_alpha` in place rather than added to `fields`.
        let alpha_marker = alpha_sentinel.to_le_bytes().to_vec();
        let alpha_offsets: Vec<_> = bytes
            .windows(alpha_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == alpha_marker).then_some(offset))
            .collect();
        assert_eq!(alpha_offsets.len(), 1, "alpha sentinel is not unique");
        let alpha_offset = alpha_offsets[0];
        bytes[alpha_offset..alpha_offset + alpha_marker.len()]
            .copy_from_slice(&prior_alpha.to_le_bytes());
        fields.push((alpha_offset + alpha_marker.len(), 2));
    }
    fields.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in fields {
        bytes.drain(offset..offset + len);
    }
    bytes
}

/// Strip the empty save-section table version 37 appends to a standalone
/// reference's own record. `without_exclusive_branches` only locates this
/// table for a reference paired with an actor, through the actor's own
/// alpha value. A fixture that materializes a bare reference, such as an
/// authored loose item never turned into an actor, calls this instead
/// before truncating the rest of the buffer to an older header. Order does
/// not matter, an empty slice is a no-op that still round trips the state.
///
/// Same cross-binary sharing as `without_exclusive_branches` above: only the
/// binaries that fixture a bare reference call this one.
#[allow(dead_code)]
pub fn without_ref_sections(world: &mut World, refs: &[GlobalFormId]) -> Vec<u8> {
    let hash = world.state_hash();
    let mut saved = Vec::new();
    for (index, id) in refs.iter().enumerate() {
        let reference = world.reference(*id).unwrap();
        let prior_alpha = reference.alpha;
        let alpha_sentinel = 800_000.0 + index as f32;
        reference.alpha = alpha_sentinel;
        saved.push((*id, prior_alpha, alpha_sentinel));
    }
    let mut bytes = world.save_state();
    for (id, prior_alpha, _) in &saved {
        world.reference(*id).unwrap().alpha = *prior_alpha;
    }
    assert_eq!(
        world.state_hash(),
        hash,
        "legacy fixture changed world state"
    );
    let mut offsets = Vec::new();
    for (_, prior_alpha, alpha_sentinel) in saved {
        let marker = alpha_sentinel.to_le_bytes().to_vec();
        let found: Vec<_> = bytes
            .windows(marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == marker).then_some(offset))
            .collect();
        assert_eq!(found.len(), 1, "reference alpha sentinel is not unique");
        let offset = found[0];
        bytes[offset..offset + marker.len()].copy_from_slice(&prior_alpha.to_le_bytes());
        offsets.push(offset + marker.len());
    }
    offsets.sort_by_key(|offset| std::cmp::Reverse(*offset));
    for offset in offsets {
        bytes.drain(offset..offset + 2);
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
    prior_alpha: f32,
    alpha_sentinel: f32,
}
