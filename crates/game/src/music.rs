// SPDX-License-Identifier: GPL-3.0-or-later

//! Record-backed selection of MUSC music types and eligible MUST tracks.

use std::collections::BTreeSet;

use soe_esm::cursor::{zstring, Fields};
use soe_esm::records::gameplay::Condition;
use soe_esm::{Error, GlobalFormId, LoadOrder, Record, Result, Tag};
use soe_papyrus::Vm;

use crate::conditions::{self, Context};
use crate::{Game, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const PALETTE: u32 = 0x23F6_78C3;
const SINGLE_TRACK: u32 = 0x6ED7_E048;
const SILENT_TRACK: u32 = 0xA1A9_C4D5;

/// MUSC behavior flags read from FNAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MusicFlags {
    pub plays_one_selection: bool,
    pub abrupt_transition: bool,
    pub cycle_tracks: bool,
    pub maintain_track_order: bool,
    pub ducks_current_track: bool,
    pub does_not_queue: bool,
}

/// The resolved playable leaf in a MUST palette, or an authored silent leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicTrack {
    pub music_type: GlobalFormId,
    pub track: GlobalFormId,
    pub path: Option<String>,
    pub duration: f32,
    pub fade_in: f32,
    pub fade_out: f32,
    pub loop_begin: Option<f32>,
    pub loop_end: Option<f32>,
    pub loop_count: u32,
    pub cue_points: Vec<f32>,
    pub flags: MusicFlags,
    pub priority: u16,
    pub ducking_db: f32,
}

/// Select one eligible music track. `draw` is a caller-owned uniform choice
/// used only where a MUSC or MUST palette offers multiple eligible tracks.
pub fn select(
    game: &mut Game,
    music_type: GlobalFormId,
    subject: GlobalFormId,
    draw: f32,
) -> Result<MusicTrack> {
    if !draw.is_finite() || !(0.0..=1.0).contains(&draw) {
        return Err(Error::Invalid(
            "music draw must be finite and in 0 to 1".into(),
        ));
    }
    let vm = &game.vm;
    let mut world = game.world_mut();
    select_with(&mut world, vm, music_type, subject, draw)
}

fn select_with(
    world: &mut World,
    vm: &Vm,
    music_type: GlobalFormId,
    subject: GlobalFormId,
    draw: f32,
) -> Result<MusicTrack> {
    let (record, file) = record(world.order(), music_type, T(b"MUSC"))?;
    let flags_value = field(record, T(b"FNAM"), 4)?.u32()?;
    let flags = MusicFlags {
        plays_one_selection: flags_value & 1 != 0,
        abrupt_transition: flags_value & 2 != 0,
        cycle_tracks: flags_value & 4 != 0,
        maintain_track_order: flags_value & 8 != 0,
        ducks_current_track: flags_value & 0x20 != 0,
        does_not_queue: flags_value & 0x40 != 0,
    };
    let mut data = field(record, T(b"PNAM"), 4)?;
    let priority = data.u16()?;
    let ducking_db = f32::from(data.u16()?) / 100.0;
    let fade_in = read_float(record, T(b"WNAM"), 0.0)?;
    let candidates = links(world.order(), record, file, T(b"TNAM"))?;
    if candidates.is_empty() {
        return Err(Error::Invalid(format!(
            "music type {music_type} has no tracks"
        )));
    }
    let mut eligible = Vec::new();
    for id in candidates {
        if track_eligible(world, vm, id, subject)? {
            eligible.push(id);
        }
    }
    if eligible.is_empty() {
        return Err(Error::Invalid(format!(
            "music type {music_type} has no eligible tracks"
        )));
    }
    let track = choose(&eligible, draw);
    let mut visited = BTreeSet::new();
    select_leaf(
        world,
        vm,
        music_type,
        track,
        subject,
        draw,
        flags,
        priority,
        ducking_db,
        fade_in,
        &mut visited,
    )
}

#[allow(clippy::too_many_arguments)]
fn select_leaf(
    world: &mut World,
    vm: &Vm,
    music_type: GlobalFormId,
    id: GlobalFormId,
    subject: GlobalFormId,
    draw: f32,
    flags: MusicFlags,
    priority: u16,
    ducking_db: f32,
    fade_in: f32,
    visited: &mut BTreeSet<GlobalFormId>,
) -> Result<MusicTrack> {
    if !visited.insert(id) {
        return Err(Error::Invalid(format!("MUST palette cycle at {id}")));
    }
    if !track_eligible(world, vm, id, subject)? {
        return Err(Error::Invalid(format!("MUST track {id} is not eligible")));
    }
    let (record, file) = record(world.order(), id, T(b"MUST"))?;
    let kind = field(record, T(b"CNAM"), 4)?.u32()?;
    if kind == PALETTE {
        let children = links(world.order(), record, file, T(b"SNAM"))?;
        let mut eligible = Vec::new();
        for child in children {
            if track_eligible(world, vm, child, subject)? {
                eligible.push(child);
            }
        }
        if eligible.is_empty() {
            return Err(Error::Invalid(format!(
                "MUST palette {id} has no eligible tracks"
            )));
        }
        return select_leaf(
            world,
            vm,
            music_type,
            choose(&eligible, draw),
            subject,
            draw,
            flags,
            priority,
            ducking_db,
            fade_in,
            visited,
        );
    }
    if kind != SINGLE_TRACK && kind != SILENT_TRACK {
        return Err(Error::Invalid(format!("unknown MUST track type {kind:#x}")));
    }
    let path = if kind == SILENT_TRACK {
        None
    } else {
        let path = record.find(T(b"ANAM")).map(zstring).unwrap_or_default();
        if path.is_empty() {
            return Err(Error::Invalid(format!("MUST track {id} has no filename")));
        }
        Some(soe_bsa::normalize(&path))
    };
    let loop_data = record.find(T(b"LNAM"));
    let (loop_begin, loop_end, loop_count) = if let Some(raw) = loop_data {
        let mut fields = Fields::new(record.tag, T(b"LNAM"), raw);
        fields.exact(12)?;
        (Some(fields.f32()?), Some(fields.f32()?), fields.u32()?)
    } else {
        (None, None, 0)
    };
    let mut cue_points = Vec::new();
    for raw in record.all(T(b"FNAM")) {
        let (chunks, remainder) = raw.as_chunks::<4>();
        if !remainder.is_empty() {
            return Err(Error::Invalid(format!("invalid MUST cue points at {id}")));
        }
        cue_points.extend(chunks.iter().map(|bytes| f32::from_le_bytes(*bytes)));
    }
    Ok(MusicTrack {
        music_type,
        track: id,
        path,
        duration: read_float(record, T(b"FLTV"), 0.0)?,
        fade_in: if flags.abrupt_transition {
            0.0
        } else {
            fade_in
        },
        fade_out: read_float(record, T(b"DNAM"), 0.0)?,
        loop_begin,
        loop_end,
        loop_count,
        cue_points,
        flags,
        priority,
        ducking_db,
    })
}

fn track_eligible(
    world: &mut World,
    vm: &Vm,
    id: GlobalFormId,
    subject: GlobalFormId,
) -> Result<bool> {
    let (record, _) = record(world.order(), id, T(b"MUST"))?;
    let mut conditions = Vec::new();
    for subrecord in &record.subrecords {
        Condition::push(record, subrecord, &mut conditions)?;
    }
    conditions::evaluate(
        world,
        vm,
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
        &conditions,
    )
    .map_err(|error| Error::Invalid(format!("music condition at {id}: {error}")))
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

fn field(record: &Record, tag: Tag, size: usize) -> Result<Fields<'_>> {
    let raw = record.required(tag)?;
    let fields = Fields::new(record.tag, tag, raw);
    fields.exact(size)?;
    Ok(fields)
}

fn read_float(record: &Record, tag: Tag, default: f32) -> Result<f32> {
    let Some(raw) = record.find(tag) else {
        return Ok(default);
    };
    let mut fields = Fields::new(record.tag, tag, raw);
    fields.exact(4)?;
    let value = fields.f32()?;
    if !value.is_finite() || value < 0.0 {
        return Err(Error::Invalid(format!("invalid {tag} value")));
    }
    Ok(value)
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
            let raw_id = u32::from_le_bytes(*entry);
            if raw_id != 0 {
                ids.push(order.resolve(file, raw_id));
            }
        }
    }
    Ok(ids)
}

fn choose(ids: &[GlobalFormId], draw: f32) -> GlobalFormId {
    let index = ((draw * ids.len() as f32) as usize).min(ids.len() - 1);
    ids[index]
}
