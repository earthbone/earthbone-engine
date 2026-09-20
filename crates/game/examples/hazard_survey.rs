// SPDX-License-Identifier: GPL-3.0-or-later

//! Survey Spawn Hazard dependencies in the five base masters with SKYRIM_DATA set.
//! Diagnostic HAZD and PROJ reads follow xEdit wbDefinitionsTES5.pas and Mutagen
//! Hazard.xml, using the existing bounds-checked record reader and field cursor.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::cursor::Fields;
use soe_esm::records::gameplay::{MagicEffect, Spell};
use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let data = std::path::PathBuf::from(std::env::var("SKYRIM_DATA")?);
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order.add(name, Plugin::parse(&std::fs::read(data.join(name))?)?)?;
    }
    for tag in [b"HAZD", b"PHZD", b"PROJ"] {
        let count = order
            .winners_of(Tag::new(tag))
            .filter(|(_, record)| record.flags & 0x20 == 0)
            .count();
        println!("{} winners: {count}", Tag::new(tag));
    }

    let mut effect_count = 0;
    let mut spawn_effects = BTreeSet::new();
    let mut hazards = BTreeSet::new();
    let mut effect_modes = BTreeMap::<(u32, u32), usize>::new();
    for (id, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        effect_count += 1;
        let owner = order.winner_ref(id).ok_or("missing MGEF owner")?;
        let effect =
            MagicEffect::from_record(record, order.file(owner.file).plugin.header.is_localized())?;
        if effect.data.archetype != 40 {
            continue;
        }
        spawn_effects.insert(id);
        *effect_modes
            .entry((effect.data.cast_type, effect.data.delivery))
            .or_default() += 1;
        println!("MGEF {id} {:?}: {:?}", effect.editor_id, effect.data);
        println!("  conditions: {:?}", effect.conditions);
        let mut fields = data_fields(record)?;
        fields.skip(72)?;
        let projectile = fields.u32()?;
        if projectile != 0 {
            let projectile = order.resolve(owner.file, projectile);
            let record = order.winner(projectile).ok_or("missing projectile")?;
            if record.tag != Tag::new(b"PROJ") || record.flags & 0x20 != 0 {
                return Err("invalid projectile link".into());
            }
            let mut fields = data_fields(record)?;
            let flags = fields.u16()?;
            let kind = fields.u16()?;
            println!(
                "  PROJ {projectile} {:?}: flags={flags:#x} type={kind}",
                record.editor_id()
            );
        }
        if effect.data.associated_item == 0 {
            return Err("Spawn Hazard effect has no associated hazard".into());
        }
        let hazard = order.resolve(owner.file, effect.data.associated_item);
        hazards.insert(hazard);
        inspect_hazard(&order, hazard)?;
    }
    let mut spell_count = 0;
    let mut associated = Vec::new();
    let mut spell_modes = BTreeMap::<(u32, u32), usize>::new();
    for (id, record) in order.winners_of(Tag::new(b"SPEL")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        spell_count += 1;
        let owner = order.winner_ref(id).ok_or("missing SPEL owner")?;
        let spell =
            Spell::from_record(record, order.file(owner.file).plugin.header.is_localized())?;
        if spell
            .effects
            .iter()
            .any(|entry| spawn_effects.contains(&order.resolve(owner.file, entry.base_effect)))
        {
            associated.push(id);
            *spell_modes
                .entry((spell.data.cast_type, spell.data.delivery))
                .or_default() += 1;
            inspect_spell(&order, id)?;
        }
    }
    println!(
        "survey: {effect_count} MGEF, {spell_count} SPEL, {} Spawn Hazard MGEF, {} linked HAZD, {} associated SPEL",
        spawn_effects.len(), hazards.len(), associated.len()
    );
    println!("MGEF (cast, delivery): {effect_modes:?}");
    println!("SPEL (cast, delivery): {spell_modes:?}");
    if spawn_effects.is_empty() || associated.is_empty() {
        return Err("no Spawn Hazard records found in the five-master corpus".into());
    }
    let mut game = Game::new(order);
    let player = game.player();
    // Actor lookup materializes state, so finish fixture setup before snapshots.
    game.world_mut()
        .actor(player)
        .ok_or("missing player actor")?;
    let mut rejected = BTreeMap::<String, usize>::new();
    let mut accepted = 0;
    for spell in associated {
        let shared = game.shared_world();
        let before = shared.borrow().save_state();
        let result = shared.borrow_mut().cast_spell(
            spell,
            player,
            Some(player),
            Some(player),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(ids) => {
                accepted += 1;
                println!("cast {spell}: {} applications", ids.len());
            }
            Err(error) => {
                let unchanged = before == shared.borrow().save_state();
                println!("cast {spell}: {error}, world unchanged={unchanged}");
                if !unchanged {
                    return Err("rejected cast changed world state".into());
                }
                *rejected.entry(error.to_string()).or_default() += 1;
            }
        }
    }
    println!("casts: accepted={accepted} rejected={rejected:?}");
    Ok(())
}

fn data_fields(record: &Record) -> Result<Fields<'_>> {
    let tag = Tag::new(b"DATA");
    Ok(Fields::new(
        record.tag,
        tag,
        record.find(tag).ok_or("missing DATA")?,
    ))
}

fn inspect_hazard(order: &LoadOrder, id: GlobalFormId) -> Result<()> {
    let owner = order.winner_ref(id).ok_or("missing HAZD owner")?;
    let record = order.record(owner);
    if record.tag != Tag::new(b"HAZD") || record.flags & 0x20 != 0 {
        return Err("invalid associated hazard link".into());
    }
    let mut fields = data_fields(record)?;
    fields.exact(40)?;
    let limit = fields.u32()?;
    let radius = fields.f32()?;
    let lifetime = fields.f32()?;
    let image_space_radius = fields.f32()?;
    let target_interval = fields.f32()?;
    let flags = fields.u32()?;
    let spell = fields.u32()?;
    let light = fields.u32()?;
    let impact = fields.u32()?;
    let sound = fields.u32()?;
    println!(
        "  HAZD {id} {:?}: limit={limit} radius={radius} lifetime={lifetime} image_space_radius={image_space_radius} target_interval={target_interval} flags={flags:#x}",
        record.editor_id()
    );
    for (name, raw) in [("light", light), ("impact", impact), ("sound", sound)] {
        if raw != 0 {
            let link = order.resolve(owner.file, raw);
            let record = order.winner(link).ok_or("missing hazard asset")?;
            println!("    {name} {link} {} {:?}", record.tag, record.editor_id());
        }
    }
    if spell != 0 {
        inspect_spell(order, order.resolve(owner.file, spell))?;
    }
    Ok(())
}

fn inspect_spell(order: &LoadOrder, id: GlobalFormId) -> Result<()> {
    let owner = order.winner_ref(id).ok_or("missing SPEL owner")?;
    let spell = Spell::from_record(
        order.record(owner),
        order.file(owner.file).plugin.header.is_localized(),
    )?;
    println!("  SPEL {id} {:?}: {:?}", spell.editor_id, spell.data);
    for entry in &spell.effects {
        let effect = order.resolve(owner.file, entry.base_effect);
        let effect_owner = order.winner_ref(effect).ok_or("missing effect owner")?;
        let base = MagicEffect::from_record(
            order.record(effect_owner),
            order.file(effect_owner.file).plugin.header.is_localized(),
        )?;
        println!("    MGEF {effect} {:?}: {:?}", base.editor_id, base.data);
        println!("    EFIT {:?}", entry.data);
        println!("    spell conditions: {:?}", entry.conditions);
        println!("    effect conditions: {:?}", base.conditions);
    }
    Ok(())
}
