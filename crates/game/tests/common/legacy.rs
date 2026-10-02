// SPDX-License-Identifier: GPL-3.0-or-later

//! Legacy SOEW fixtures from world/save.rs and world/save_sections.rs.

use soe_esm::GlobalFormId;
use soe_game::world::World;

/// Strip uniquely marked actor fields before a fixture applies its older layout.
/// Callers supply every initialized actor and restore the requested old header.
/// Strips the exclusive dialogue branch version 24 added, the stagger end time
/// version 29 added, the witnessed crime set version 31 added, the hand spell
/// fields version 32 added and the MAG-57 actor section, so fixtures built
/// against an older header see none of those fields' bytes. Also strips the
/// actor's named section table and the reference's empty save-section table
/// version 37 appends to those records and, since `World::actor`
/// requires a paired reference (`self.reference(id)` with `is_actor` set), to
/// that reference's record too. It also reduces world-level section payloads
/// to an empty table before measuring the legacy tail.
///
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
        // Remove this actor's complete section table, including any named
        // sections added after the legacy version being simulated.
        let actor_sections = right_spell_offsets[0] + right_spell_marker.len();
        fields.push((actor_sections, section_table_len(&bytes, actor_sections)));

        // `alpha` itself is not version gated, so unlike the fields above
        // it must stay in the stream with its true value. Only the named
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
        let reference_sections = alpha_offset + alpha_marker.len();
        fields.push((
            reference_sections,
            section_table_len(&bytes, reference_sections),
        ));
    }
    fields.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in fields {
        bytes.drain(offset..offset + len);
    }
    normalize_world_sections(&mut bytes);
    bytes
}

/// Strip the post-version-33 AI section and the paired reference sections
/// while retaining the actor fields used by version 28 and later fixtures.
#[allow(dead_code)]
pub fn without_recent_ai_fields(world: &mut World, actors: &[GlobalFormId]) -> Vec<u8> {
    without_ref_sections(world, actors)
}

/// Strip the named save-section table version 37 appends to a standalone
/// reference's own record.
/// `without_exclusive_branches` only locates this
/// table for a reference paired with an actor, through the actor's own
/// alpha value. A fixture that materializes a bare reference, such as an
/// authored loose item never turned into an actor, calls this instead
/// before truncating the rest of the buffer to an older header. Order does
/// not matter, a table with no registered sections still round trips the state.
///
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
        let section_offset = offset + marker.len();
        offsets.push((section_offset, section_table_len(&bytes, section_offset)));
    }
    offsets.sort_by_key(|(offset, _)| std::cmp::Reverse(*offset));
    for (offset, len) in offsets {
        bytes.drain(offset..offset + len);
    }
    normalize_world_sections(&mut bytes);
    bytes
}

#[allow(dead_code)]
pub fn section_table_len(bytes: &[u8], start: usize) -> usize {
    let count = usize::from(u16::from_le_bytes([bytes[start], bytes[start + 1]]));
    let mut cursor = start + 2;
    for _ in 0..count {
        let name_len = usize::from(bytes[cursor]);
        cursor += 1 + name_len;
        let payload_len = u32::from_le_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        cursor += 4 + payload_len;
    }
    cursor - start
}

/// Retain the version 37 fixed bodies and replace every named table with an
/// empty table, matching the writer at the save freeze, commit 72add3f8.
#[allow(dead_code)]
pub fn version_37_at_freeze(world: &mut World, refs: &[GlobalFormId]) -> Vec<u8> {
    version_at_freeze(world, refs, 37)
}

/// Retain the version 36 fixed bodies without named tables or an empty
/// detection block. This fixture requires empty detection pools.
#[allow(dead_code)]
pub fn version_36_at_freeze(world: &mut World, refs: &[GlobalFormId]) -> Vec<u8> {
    version_at_freeze(world, refs, 36)
}

fn version_at_freeze(world: &mut World, refs: &[GlobalFormId], version: u16) -> Vec<u8> {
    let before = world.save_state();
    let mut actors = Vec::new();
    let mut references = Vec::new();
    for (index, id) in refs.iter().enumerate() {
        if let Some(actor) = world.actor_ref(*id) {
            let prior = actor.right_spell;
            let sentinel = GlobalFormId {
                file: u16::MAX,
                object: 0xB000_0000 + index as u32,
            };
            world.actor(*id).unwrap().right_spell = Some(sentinel);
            actors.push((*id, prior, sentinel));
        }
        let reference = world.reference(*id).unwrap();
        let prior = reference.alpha;
        let sentinel = 900_000.0 + index as f32;
        reference.alpha = sentinel;
        references.push((*id, prior, sentinel));
    }
    let mut bytes = world.save_state();
    for (id, prior, _) in &actors {
        world.actor(*id).unwrap().right_spell = *prior;
    }
    for (id, prior, _) in &references {
        world.reference(*id).unwrap().alpha = *prior;
    }
    assert_eq!(world.save_state(), before, "fixture preserves source state");
    let mut replacements = Vec::new();
    for (_, prior, sentinel) in actors {
        let mut marker = vec![1];
        marker.extend_from_slice(&sentinel.file.to_le_bytes());
        marker.extend_from_slice(&sentinel.object.to_le_bytes());
        let start = unique_offset(&bytes, &marker);
        let table_start = start + marker.len();
        let end = table_start + section_table_len(&bytes, table_start);
        let mut replacement = vec![u8::from(prior.is_some())];
        if let Some(prior) = prior {
            replacement.extend_from_slice(&prior.file.to_le_bytes());
            replacement.extend_from_slice(&prior.object.to_le_bytes());
        }
        if version == 37 {
            replacement.extend_from_slice(&0_u16.to_le_bytes());
        }
        replacements.push((start, end, replacement));
    }
    for (_, prior, sentinel) in references {
        let start = unique_offset(&bytes, &sentinel.to_le_bytes());
        let table_start = start + std::mem::size_of::<f32>();
        let end = table_start + section_table_len(&bytes, table_start);
        let mut replacement = prior.to_le_bytes().to_vec();
        if version == 37 {
            replacement.extend_from_slice(&0_u16.to_le_bytes());
        }
        replacements.push((start, end, replacement));
    }
    replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    for (start, end, replacement) in replacements {
        bytes.splice(start..end, replacement);
    }
    if version == 37 {
        empty_world_sections(&mut bytes);
    } else {
        strip_world_sections(&mut bytes);
        assert!(
            bytes.ends_with(&0_u32.to_le_bytes()),
            "empty detection fixture"
        );
        bytes.truncate(bytes.len() - std::mem::size_of::<u32>());
    }
    bytes[4..6].copy_from_slice(&version.to_le_bytes());
    bytes
}

/// Normalize a version 37 fixture to an empty world-level section table.
#[allow(dead_code)]
pub fn empty_world_sections(bytes: &mut Vec<u8>) {
    normalize_world_sections(bytes);
}

#[allow(dead_code)]
pub fn world_section_growth(bytes: &[u8]) -> usize {
    let mut normalized = bytes.to_vec();
    normalize_world_sections(&mut normalized);
    bytes.len() - normalized.len()
}

#[allow(unused_imports)]
pub use crate::common::save_tail::{truncate_to_version, version_tail_len};

#[allow(dead_code)]
pub fn unique_offset(bytes: &[u8], marker: &[u8]) -> usize {
    let offsets: Vec<_> = bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == marker).then_some(offset))
        .collect();
    assert_eq!(offsets.len(), 1, "save marker is not unique");
    offsets[0]
}

#[allow(dead_code)]
pub fn common_suffix_len(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

/// Remove the world save-section table appended at the end of a version 37
/// save when constructing a pre-version-37 fixture.
#[allow(dead_code)]
pub fn strip_world_sections(bytes: &mut Vec<u8>) -> usize {
    let start = (0..bytes.len())
        .find(|start| section_table_end(bytes, *start) == Some(bytes.len()))
        .expect("world save-section table is present at the end of the save");
    let length = bytes.len() - start;
    bytes.truncate(start);
    length
}

pub fn normalize_world_sections(bytes: &mut Vec<u8>) {
    let _ = strip_world_sections(bytes);
    bytes.extend_from_slice(&0u16.to_le_bytes());
}

#[allow(dead_code)]
fn section_table_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    let count = usize::from(u16::from_le_bytes([
        *bytes.get(cursor)?,
        *bytes.get(cursor.checked_add(1)?)?,
    ]));
    cursor = cursor.checked_add(2)?;
    for _ in 0..count {
        let name_len = usize::from(*bytes.get(cursor)?);
        let name_start = cursor.checked_add(1)?;
        cursor = name_start.checked_add(name_len)?;
        let name = bytes.get(name_start..cursor)?;
        if name.is_empty() || !name.iter().all(u8::is_ascii_graphic) {
            return None;
        }
        let payload_len = usize::try_from(u32::from_le_bytes([
            *bytes.get(cursor)?,
            *bytes.get(cursor.checked_add(1)?)?,
            *bytes.get(cursor.checked_add(2)?)?,
            *bytes.get(cursor.checked_add(3)?)?,
        ]))
        .ok()?;
        cursor = cursor.checked_add(4)?.checked_add(payload_len)?;
        if cursor > bytes.len() {
            return None;
        }
    }
    Some(cursor)
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
