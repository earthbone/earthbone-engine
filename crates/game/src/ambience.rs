// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell music descriptors follow xEdit's CELL, REGN, MUSC and MUST layouts
//! and UESP's corresponding record pages. Playback is owned by the host.

use soe_esm::cursor::zstring;
use soe_esm::records::cell::Cell;
use soe_esm::{Error, GlobalFormId, Result, Tag};

use crate::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
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

/// Resolve supported direct single tracks for a cell.
///
/// A cell's XCMO wins over regional RDMO. Regions are visited in their CELL
/// order and the first usable regional music type is selected.
///
/// # Errors
/// Returns malformed CELL or track field errors.
pub fn tracks_for_cell(world: &World, cell: GlobalFormId) -> Result<Vec<Track>> {
    let record = world
        .record(cell)
        .filter(|record| record.tag == T(b"CELL"))
        .ok_or_else(|| Error::Invalid(format!("missing CELL {cell}")))?;
    let cell_record = Cell::from_record(record, false)?;
    let music = if let Some(raw) = cell_record.music {
        world.resolve_from(cell, raw)
    } else {
        cell_record.regions.iter().find_map(|raw_region| {
            let region = world.resolve_from(cell, *raw_region)?;
            let record = world.record(region)?;
            if record.tag != T(b"REGN") {
                return None;
            }
            let mut sound_entry = false;
            for subrecord in &record.subrecords {
                if subrecord.tag == T(b"RDAT") {
                    sound_entry = subrecord.data.len() >= 4
                        && u32::from_le_bytes(subrecord.data[..4].try_into().ok()?) == 7;
                } else if sound_entry && subrecord.tag == T(b"RDMO") {
                    let raw = u32::from_le_bytes(subrecord.data.get(..4)?.try_into().ok()?);
                    return world.resolve_from(region, raw);
                }
            }
            None
        })
    };
    let Some(descriptor) = music else {
        return Ok(Vec::new());
    };
    tracks_for_music(world, cell, descriptor)
}

fn tracks_for_music(world: &World, cell: GlobalFormId, music: GlobalFormId) -> Result<Vec<Track>> {
    let record = world
        .record(music)
        .filter(|record| record.tag == T(b"MUSC"))
        .ok_or_else(|| Error::Invalid(format!("missing MUSC {music}")))?;
    let mut tracks = Vec::new();
    for raw_track in record.all(T(b"TNAM")) {
        if raw_track.len() != 4 {
            return Err(Error::Invalid(format!("invalid MUSC.TNAM at {music}")));
        }
        let id = world.resolve_from(
            music,
            u32::from_le_bytes(
                raw_track
                    .try_into()
                    .map_err(|_| Error::Invalid(format!("invalid MUSC.TNAM at {music}")))?,
            ),
        );
        let Some(id) = id else { continue };
        let Some(track) = world.record(id).filter(|record| record.tag == T(b"MUST")) else {
            continue;
        };
        let kind = track
            .find(T(b"CNAM"))
            .filter(|data| data.len() == 4)
            .map(|data| u32::from_le_bytes(data.try_into().unwrap_or([0; 4])));
        if kind != Some(SINGLE_TRACK) {
            continue;
        }
        let Some(path) = track
            .find(T(b"ANAM"))
            .map(zstring)
            .filter(|path| !path.is_empty())
        else {
            continue;
        };
        tracks.push(Track {
            cell,
            descriptor: music,
            path,
        });
    }
    Ok(tracks)
}
