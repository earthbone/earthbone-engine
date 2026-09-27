// SPDX-License-Identifier: GPL-3.0-or-later

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips the exclusive dialogue branch version 24 added, the stagger end time
/// version 29 added, the witnessed crime set version 31 added and the hand
/// spell fields version 32 added. It also strips AI fields added in versions
/// 37, 41, 42 and 43, so older headers see their matching actor layout.
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
        let bed_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xA000_0000 + index as u32,
        };
        let found_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xA100_0000 + index as u32,
        };
        let prior_bed = actor.ai.bed.replace(bed_sentinel);
        let prior_force_greet = std::mem::replace(&mut actor.ai.force_greet_started, true);
        let prior_activate = std::mem::replace(&mut actor.ai.activate_started, true);
        let prior_found = actor.ai.found_reference.replace(found_sentinel);
        let prior_dialogue_activate =
            std::mem::replace(&mut actor.ai.dialogue_activate_started, true);
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
            prior_bed,
            bed_sentinel,
            prior_force_greet,
            prior_activate,
            prior_found,
            found_sentinel,
            prior_dialogue_activate,
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
        actor.ai.bed = entry.prior_bed;
        actor.ai.force_greet_started = entry.prior_force_greet;
        actor.ai.activate_started = entry.prior_activate;
        actor.ai.found_reference = entry.prior_found;
        actor.ai.dialogue_activate_started = entry.prior_dialogue_activate;
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
        prior_bed,
        bed_sentinel,
        found_sentinel,
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
        fields.push((offsets[0], marker.len(), Vec::new()));

        let mut stagger_marker = vec![1];
        stagger_marker.extend_from_slice(&stagger_sentinel.to_le_bytes());
        let stagger_offsets: Vec<_> = bytes
            .windows(stagger_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == stagger_marker).then_some(offset))
            .collect();
        assert_eq!(stagger_offsets.len(), 1, "stagger sentinel is not unique");
        fields.push((stagger_offsets[0], stagger_marker.len(), Vec::new()));

        let mut witness_marker = 1u32.to_le_bytes().to_vec();
        witness_marker.extend_from_slice(&witness_sentinel.file.to_le_bytes());
        witness_marker.extend_from_slice(&witness_sentinel.object.to_le_bytes());
        let witness_offsets: Vec<_> = bytes
            .windows(witness_marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == witness_marker).then_some(offset))
            .collect();
        assert_eq!(witness_offsets.len(), 1, "witness sentinel is not unique");
        fields.push((witness_offsets[0], witness_marker.len(), Vec::new()));

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
        fields.push((left_spell_offsets[0], left_spell_marker.len(), Vec::new()));

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
        fields.push((right_spell_offsets[0], right_spell_marker.len(), Vec::new()));

        let mut found_marker = optional_form_bytes(Some(found_sentinel));
        found_marker.push(1);
        let found_at = bytes
            .windows(found_marker.len())
            .position(|window| window == found_marker)
            .expect("Find and DialogueActivate sentinel is serialized");
        fields.push((found_at, found_marker.len(), Vec::new()));

        let bed_marker = optional_form_bytes(Some(bed_sentinel));
        let bed_at = bytes
            .windows(bed_marker.len())
            .position(|window| window == bed_marker)
            .expect("bed sentinel is serialized");
        assert_eq!(&bytes[bed_at - 3..bed_at - 1], &[1, 1]);
        fields.push((bed_at - 3, 2, Vec::new()));
        fields.push((bed_at, bed_marker.len(), optional_form_bytes(prior_bed)));
    }
    fields.sort_by_key(|(offset, _, _)| std::cmp::Reverse(*offset));
    for (offset, len, replacement) in fields {
        bytes.splice(offset..offset + len, replacement);
    }
    bytes
}

fn optional_form_bytes(id: Option<GlobalFormId>) -> Vec<u8> {
    let mut bytes = vec![u8::from(id.is_some())];
    if let Some(id) = id {
        bytes.extend_from_slice(&id.file.to_le_bytes());
        bytes.extend_from_slice(&id.object.to_le_bytes());
    }
    bytes
}

/// Remove the AI fields added after SOEW version 33 while keeping other state.
#[allow(dead_code)]
pub fn without_recent_ai_fields(world: &mut World, actors: &[GlobalFormId]) -> Vec<u8> {
    let hash = world.state_hash();
    let mut saved = Vec::new();
    for (index, id) in actors.iter().enumerate() {
        let actor = world
            .actor(*id)
            .expect("legacy fixture actor is initialized");
        let bed_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xA200_0000 + index as u32,
        };
        let found_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xA300_0000 + index as u32,
        };
        let prior_bed = actor.ai.bed.replace(bed_sentinel);
        let prior_force = std::mem::replace(&mut actor.ai.force_greet_started, true);
        let prior_activate = std::mem::replace(&mut actor.ai.activate_started, true);
        let prior_found = actor.ai.found_reference.replace(found_sentinel);
        let prior_dialogue = std::mem::replace(&mut actor.ai.dialogue_activate_started, true);
        saved.push((
            *id,
            bed_sentinel,
            prior_bed,
            prior_force,
            prior_activate,
            found_sentinel,
            prior_found,
            prior_dialogue,
        ));
    }
    let mut bytes = world.save_state();
    let mut fields = Vec::new();
    for (
        id,
        bed_sentinel,
        prior_bed,
        prior_force,
        prior_activate,
        found_sentinel,
        prior_found,
        prior_dialogue,
    ) in saved
    {
        let actor = world
            .actor(id)
            .expect("legacy fixture actor is initialized");
        actor.ai.bed = prior_bed;
        actor.ai.force_greet_started = prior_force;
        actor.ai.activate_started = prior_activate;
        actor.ai.found_reference = prior_found;
        actor.ai.dialogue_activate_started = prior_dialogue;

        let mut found_marker = optional_form_bytes(Some(found_sentinel));
        found_marker.push(1);
        let found_at = bytes
            .windows(found_marker.len())
            .position(|window| window == found_marker)
            .expect("Find and DialogueActivate sentinel is serialized");
        fields.push((found_at, found_marker.len(), Vec::new()));

        let bed_marker = optional_form_bytes(Some(bed_sentinel));
        let bed_at = bytes
            .windows(bed_marker.len())
            .position(|window| window == bed_marker)
            .expect("bed sentinel is serialized");
        assert_eq!(&bytes[bed_at - 3..bed_at - 1], &[1, 1]);
        fields.push((bed_at - 3, 2, Vec::new()));
        fields.push((bed_at, bed_marker.len(), optional_form_bytes(prior_bed)));
    }
    assert_eq!(
        world.state_hash(),
        hash,
        "legacy fixture changed world state"
    );
    fields.sort_by_key(|(offset, _, _)| std::cmp::Reverse(*offset));
    for (offset, len, replacement) in fields {
        bytes.splice(offset..offset + len, replacement);
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
    prior_bed: Option<GlobalFormId>,
    bed_sentinel: GlobalFormId,
    prior_force_greet: bool,
    prior_activate: bool,
    prior_found: Option<GlobalFormId>,
    found_sentinel: GlobalFormId,
    prior_dialogue_activate: bool,
}
