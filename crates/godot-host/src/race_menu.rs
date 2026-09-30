// SPDX-License-Identifier: GPL-3.0-or-later

//! Playable character races come from winning RACE records. xEdit defines the
//! DATA playable flag and HDPT defines the sex and playability flags.

use soe_esm::{GlobalFormId, Tag};
use soe_game::Game;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub race: GlobalFormId,
    pub name: String,
}

/// List non-deleted playable races in resolved FormID order.
pub fn entries(game: &Game) -> Vec<Entry> {
    let world = game.world();
    let mut entries: Vec<_> = world
        .order()
        .winners_of(Tag(*b"RACE"))
        .filter_map(|(race, record)| {
            let data = record.find(Tag(*b"DATA"))?;
            let flags = data.get(32..36)?;
            let playable = u32::from_le_bytes(flags.try_into().ok()?) & 1 != 0;
            (!record.is_deleted() && playable).then(|| Entry {
                race,
                name: world.name_of(race).unwrap_or_else(|| format!("{race}")),
            })
        })
        .collect();
    entries.sort_by_key(|entry| entry.race);
    entries
}
