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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadPartEntry {
    pub part_type: u32,
    pub form: GlobalFormId,
    pub name: String,
}

/// Render the selected records and morph values in the race menu preview line.
pub fn preview_summary(
    game: &Game,
    appearance: &soe_game::appearance::PlayerAppearance,
    parts: &[HeadPartEntry],
) -> String {
    let world = game.world();
    let mut selected_parts = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    for (part_type, form) in &appearance.head_parts {
        let Some(entry) = parts.iter().find(|entry| entry.form == *form) else {
            continue;
        };
        selected_parts.push(format!("{part_type}: {}", entry.name));
        attached_head_parts(&world, *form, &mut visited, &mut selected_parts);
    }
    let selected_morphs = ["Face", "Nose", "Jaw"]
        .iter()
        .map(|name| {
            format!(
                "{name} {:.2}",
                appearance.morphs.get(*name).copied().unwrap_or(0.0)
            )
        })
        .collect::<Vec<_>>();
    format!(
        "Preview: {} | {}",
        selected_parts.join(", "),
        selected_morphs.join(", ")
    )
}

fn attached_head_parts(
    world: &soe_game::World,
    form: GlobalFormId,
    visited: &mut std::collections::BTreeSet<GlobalFormId>,
    names: &mut Vec<String>,
) {
    if !visited.insert(form) {
        return;
    }
    let Some(record) = world.order().winner(form) else {
        return;
    };
    let Some(file) = world
        .order()
        .winner_ref(form)
        .map(|reference| reference.file)
    else {
        return;
    };
    for raw in record
        .all(Tag(*b"HNAM"))
        .filter(|bytes| bytes.len() == 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    {
        let attached = world.order().resolve(file, raw);
        let Some(attached_record) = world.order().winner(attached) else {
            continue;
        };
        if attached_record.tag != Tag(*b"HDPT") || attached_record.is_deleted() {
            continue;
        }
        names.push(
            world
                .name_of(attached)
                .unwrap_or_else(|| format!("{attached}")),
        );
        attached_head_parts(world, attached, visited, names);
    }
}

/// Discard a race-menu draft without changing the persisted appearance.
pub fn discard_draft(draft: &mut Option<soe_game::appearance::PlayerAppearance>) {
    *draft = None;
}

/// List playable head parts that support the selected sex, in FormID order.
pub fn head_parts(game: &Game, race: GlobalFormId, female: bool) -> Vec<HeadPartEntry> {
    let world = game.world();
    let Some(race_record) = world.order().winner(race) else {
        return Vec::new();
    };
    let Some(race_file) = world
        .order()
        .winner_ref(race)
        .map(|reference| reference.file)
    else {
        return Vec::new();
    };
    let hairs = allowed_head_parts(race_record, Tag(*b"HNAM"), &world, race_file);
    let eyes = allowed_head_parts(race_record, Tag(*b"ENAM"), &world, race_file);
    let mut entries = Vec::new();
    for (form, record) in world.order().winners_of(Tag(*b"HDPT")) {
        if record.is_deleted() {
            continue;
        }
        let Some(flags) = record.find(Tag(*b"DATA")).and_then(|bytes| bytes.first()) else {
            continue;
        };
        let sex_flag = if female { 4 } else { 2 };
        if flags & 1 == 0 || flags & sex_flag == 0 {
            continue;
        }
        let Some(part_type) = record
            .find(Tag(*b"PNAM"))
            .filter(|bytes| bytes.len() == 4)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u32::from_le_bytes)
        else {
            continue;
        };
        if (part_type == 2 && !eyes.contains(&form)) || (part_type == 3 && !hairs.contains(&form)) {
            continue;
        }
        entries.push(HeadPartEntry {
            part_type,
            form,
            name: world.name_of(form).unwrap_or_else(|| format!("{form}")),
        });
    }
    entries.sort_by_key(|entry| entry.form);
    entries
}

fn allowed_head_parts(
    race: &soe_esm::Record,
    tag: Tag,
    world: &soe_game::World,
    file: usize,
) -> std::collections::BTreeSet<GlobalFormId> {
    race.all(tag)
        .filter(|bytes| bytes.len() == 4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .map(|raw| world.order().resolve(file, raw))
        .collect()
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
            let attributes = soe_game::race::Attributes::from_record(record).ok()?;
            (!record.is_deleted() && playable && attributes.is_some()).then(|| Entry {
                race,
                name: world.name_of(race).unwrap_or_else(|| format!("{race}")),
            })
        })
        .collect();
    entries.sort_by_key(|entry| entry.race);
    entries
}
