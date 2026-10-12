// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell audio follows xEdit's CELL, ASPC, REGN, MUSC and MUST layouts
//! and UESP's corresponding record pages. Playback is owned by the host.

use std::collections::BTreeSet;

use soe_esm::cursor::{zstring, Fields};
use soe_esm::records::cell::Cell;
use soe_esm::records::gameplay::Condition;
use soe_esm::{Error, GlobalFormId, LoadOrder, Record, Result, Tag};

use crate::conditions::{self, Context};
use crate::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const PALETTE: u32 = 0x23F6_78C3;
const SINGLE_TRACK: u32 = 0x6ED7_E048;

/// A supported audio file selected from a cell or one of its regions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// Cell whose transition owns this playback.
    pub cell: GlobalFormId,
    /// Music type that selected the track.
    pub descriptor: GlobalFormId,
    /// Relative path from MUST.ANAM.
    pub path: String,
}

/// Ambient SNDR forms from CELL/XCAS, ASPC/SNAM/RDAT and REGN/RDSA.
/// Regional entries compare their stored chance with a normalized RNG draw.
/// The caller supplies a normalized chance draw from the world's RNG.
///
/// # Errors
/// Returns malformed fields, invalid chances and missing linked forms.
pub fn descriptors_for_cell(
    world: &World,
    cell: GlobalFormId,
    weather: u8,
    draw: f32,
) -> Result<Vec<GlobalFormId>> {
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(Error::Invalid("invalid ambience chance draw".into()));
    }
    let record = world
        .record(cell)
        .filter(|record| record.tag == T(b"CELL"))
        .ok_or_else(|| Error::Invalid(format!("missing CELL {cell}")))?;
    let cell_data = Cell::from_record(record, false)?;
    let mut regions = Vec::new();
    for raw in cell_data.regions.iter().copied().filter(|raw| *raw != 0) {
        regions.push(
            world
                .resolve_from(cell, raw)
                .ok_or_else(|| Error::Invalid(format!("invalid CELL region link at {cell}")))?,
        );
    }
    let mut sounds = Vec::new();
    let acoustic_space = match cell_data.acoustic_space.filter(|raw| *raw != 0) {
        Some(raw) => Some(world.resolve_from(cell, raw).ok_or_else(|| {
            Error::Invalid(format!("invalid CELL acoustic-space link at {cell}"))
        })?),
        None => None,
    };
    if let Some(space) = acoustic_space {
        let space_record = world
            .record(space)
            .filter(|record| record.tag == T(b"ASPC"))
            .ok_or_else(|| Error::Invalid(format!("missing ASPC {space}")))?;
        for (tag, destination) in [(T(b"SNAM"), &mut sounds), (T(b"RDAT"), &mut regions)] {
            if tag == T(b"RDAT") && cell_data.flags & 1 == 0 {
                continue;
            }
            if let Some(data) = space_record.find(tag) {
                let raw = Fields::new(T(b"ASPC"), tag, data).u32()?;
                if data.len() != 4 {
                    return Err(Error::Invalid(format!("invalid ASPC.{tag} at {space}")));
                }
                if raw != 0 {
                    destination.push(
                        world.resolve_from(space, raw).ok_or_else(|| {
                            Error::Invalid(format!("invalid ASPC link at {space}"))
                        })?,
                    );
                }
            }
        }
    }
    let mut entries = Vec::new();
    for region in regions {
        let record = world
            .record(region)
            .filter(|record| record.tag == T(b"REGN"))
            .ok_or_else(|| Error::Invalid(format!("missing REGN {region}")))?;
        let mut header = None;
        for sub in &record.subrecords {
            if sub.tag == T(b"RDAT") {
                let mut fields = Fields::new(T(b"REGN"), sub.tag, &sub.data);
                let kind = fields.u32()?;
                let override_previous = fields.u8()? != 0;
                let priority = fields.u8()?;
                fields.skip(2)?;
                if fields.remaining() != 0 {
                    return Err(Error::Invalid(format!("invalid REGN.RDAT at {region}")));
                }
                header = (kind == 7).then_some((override_previous, priority));
            } else if sub.tag == T(b"RDSA") {
                if let Some((override_previous, priority)) = header {
                    let mut fields = Fields::new(T(b"REGN"), sub.tag, &sub.data);
                    let mut selected = Vec::new();
                    while fields.remaining() != 0 {
                        let raw = fields.u32()?;
                        let flags = fields.u32()?;
                        let chance = fields.f32()?;
                        if flags & !15 != 0
                            || !chance.is_finite()
                            || !(0.0..=100.0).contains(&chance)
                        {
                            return Err(Error::Invalid(format!("invalid REGN.RDSA at {region}")));
                        }
                        if raw != 0
                            && (flags == 0 || flags & u32::from(weather) != 0)
                            && draw < chance
                        {
                            selected.push(world.resolve_from(region, raw).ok_or_else(|| {
                                Error::Invalid(format!("invalid REGN sound at {region}"))
                            })?);
                        }
                    }
                    entries.push((priority, override_previous, selected));
                }
            }
        }
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    for (_, override_previous, selected) in entries {
        sounds.extend(selected);
        if override_previous {
            break;
        }
    }
    let mut seen = BTreeSet::new();
    sounds.retain(|sound| seen.insert(*sound));
    Ok(sounds)
}

/// Resolve supported single tracks with true MUST conditions for a cell.
///
/// A cell's XCMO wins over regional RDMO. Regions are visited in their CELL
/// order and the first usable regional music type is selected. Eligible
/// palette children are traversed in SNAM order.
///
/// # Errors
/// Returns malformed CELL, REGN, MUST or track link fields.
pub fn tracks_for_cell(world: &mut World, cell: GlobalFormId) -> Result<Vec<Track>> {
    let Some(descriptor) = music_type_for_cell(world, cell)? else {
        return Ok(Vec::new());
    };
    tracks_for_music(world, cell, descriptor)
}

/// Music identity from CELL/XCMO, region RDMO or the owning WRLD/ZNAM.
///
/// # Errors
/// Returns malformed cell, region or world music links.
pub fn music_type_for_cell(world: &World, cell: GlobalFormId) -> Result<Option<GlobalFormId>> {
    let record = world
        .record(cell)
        .filter(|record| record.tag == T(b"CELL"))
        .ok_or_else(|| Error::Invalid(format!("missing CELL {cell}")))?;
    let cell_record = Cell::from_record(record, false)?;
    let music = if let Some(raw) = cell_record.music {
        world.resolve_from(cell, raw)
    } else {
        regional_music(world, cell, &cell_record.regions)?
    };
    if music.is_some() || cell_record.music.is_some() {
        return Ok(music);
    }
    let Some(owner) = record
        .placement
        .world
        .and_then(|raw| world.resolve_from(cell, raw))
    else {
        return Ok(None);
    };
    let Some(data) = world
        .record(owner)
        .and_then(|record| record.find(T(b"ZNAM")))
    else {
        return Ok(None);
    };
    let mut fields = Fields::new(T(b"WRLD"), T(b"ZNAM"), data);
    fields.exact(4)?;
    let raw = fields.u32()?;
    Ok((raw != 0).then(|| world.resolve_from(owner, raw)).flatten())
}

fn regional_music(
    world: &World,
    cell: GlobalFormId,
    regions: &[u32],
) -> Result<Option<GlobalFormId>> {
    for raw_region in regions {
        let Some(region) = world.resolve_from(cell, *raw_region) else {
            continue;
        };
        let Some(record) = world
            .record(region)
            .filter(|record| record.tag == T(b"REGN"))
            .cloned()
        else {
            continue;
        };
        let mut sound_entry = false;
        for subrecord in &record.subrecords {
            if subrecord.tag == T(b"RDAT") {
                sound_entry = subrecord.data.len() >= 4
                    && u32::from_le_bytes(subrecord.data[..4].try_into().unwrap_or([0; 4])) == 7;
            } else if sound_entry && subrecord.tag == T(b"RDMO") {
                if subrecord.data.len() != 4 {
                    return Err(Error::Invalid(format!("invalid REGN.RDMO at {region}")));
                }
                let raw = u32::from_le_bytes(subrecord.data[..4].try_into().unwrap_or([0; 4]));
                if let Some(music) = world.resolve_from(region, raw) {
                    return Ok(Some(music));
                }
                break;
            }
        }
    }
    Ok(None)
}

fn tracks_for_music(
    world: &mut World,
    cell: GlobalFormId,
    music: GlobalFormId,
) -> Result<Vec<Track>> {
    let (record, file) = record(world.order(), music, T(b"MUSC"))?;
    let record = record.clone();
    let candidates = links(world.order(), &record, file, T(b"TNAM"))?;
    let mut tracks = Vec::new();
    let mut visited = BTreeSet::new();
    for id in candidates {
        collect_leaf_tracks(world, cell, music, id, &mut visited, &mut tracks)?;
    }
    Ok(tracks)
}

fn collect_leaf_tracks(
    world: &mut World,
    cell: GlobalFormId,
    descriptor: GlobalFormId,
    id: GlobalFormId,
    visited: &mut BTreeSet<GlobalFormId>,
    tracks: &mut Vec<Track>,
) -> Result<()> {
    if !visited.insert(id) {
        return Err(Error::Invalid(format!("MUST palette cycle at {id}")));
    }
    let (record, file) = record(world.order(), id, T(b"MUST"))?;
    let record = record.clone();
    if !eligible(world, id, &record)? {
        visited.remove(&id);
        return Ok(());
    }
    let mut kind = Fields::new(record.tag, T(b"CNAM"), record.required(T(b"CNAM"))?);
    kind.exact(4)?;
    let kind = kind.u32()?;
    if kind == PALETTE {
        for child in links(world.order(), &record, file, T(b"SNAM"))? {
            collect_leaf_tracks(world, cell, descriptor, child, visited, tracks)?;
        }
    } else if kind == SINGLE_TRACK {
        if let Some(path) = record
            .find(T(b"ANAM"))
            .map(zstring)
            .filter(|path| !path.is_empty())
        {
            tracks.push(Track {
                cell,
                descriptor,
                path,
            });
        }
    }
    visited.remove(&id);
    Ok(())
}

fn eligible(world: &mut World, id: GlobalFormId, record: &Record) -> Result<bool> {
    let mut conditions_list = Vec::new();
    for subrecord in &record.subrecords {
        Condition::push(record, subrecord, &mut conditions_list)?;
    }
    if conditions_list.is_empty() {
        return Ok(true);
    }
    let subject = world.player();
    conditions::evaluate_without_vm(
        world,
        Context {
            owner: id,
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        &conditions_list,
    )
    .map_err(|error| Error::Invalid(format!("MUST condition at {id}: {error}")))
}

fn links(order: &LoadOrder, record: &Record, file: usize, tag: Tag) -> Result<Vec<GlobalFormId>> {
    let mut ids = Vec::new();
    for raw in record.all(tag) {
        let (entries, remainder) = raw.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err(Error::Invalid(format!(
                "invalid {tag} link array in {}",
                record.tag
            )));
        }
        for entry in entries {
            let raw = u32::from_le_bytes(*entry);
            if raw != 0 {
                ids.push(order.resolve(file, raw));
            }
        }
    }
    Ok(ids)
}

fn record(order: &LoadOrder, id: GlobalFormId, tag: Tag) -> Result<(&Record, usize)> {
    let owner = order
        .winner_ref(id)
        .ok_or_else(|| Error::Invalid(format!("missing {tag} {id}")))?;
    let record = order.record(owner);
    if record.tag != tag || record.is_deleted() {
        return Err(Error::Invalid(format!("expected live {tag} at {id}")));
    }
    Ok((record, owner.file))
}
