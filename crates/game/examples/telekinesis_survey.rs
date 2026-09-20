// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspect Telekinesis records and the direct cast boundary in five base masters.
//! Uses the existing xEdit-backed record views. Run with SKYRIM_DATA set.

use std::collections::BTreeSet;

use soe_esm::records::gameplay::{MagicEffect, Spell};
use soe_esm::{LoadOrder, Plugin, Tag};
use soe_game::Game;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let mut effect_count = 0;
    let mut telekinesis = BTreeSet::new();
    for (id, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.flags & 0x20 != 0 {
            continue;
        }
        effect_count += 1;
        let owner = order.winner_ref(id).ok_or("missing MGEF owner")?;
        let effect =
            MagicEffect::from_record(record, order.file(owner.file).plugin.header.is_localized())?;
        if effect.data.archetype == 20 {
            telekinesis.insert(id);
            println!("MGEF {id} {:?}: {:?}", effect.editor_id, effect.data);
            println!("  conditions: {:?}", effect.conditions);
            if effect.data.equip_ability != 0 {
                let ability = order.resolve(owner.file, effect.data.equip_ability);
                inspect_ability(&order, ability)?;
            }
        }
    }
    let mut spell_count = 0;
    let mut associated = Vec::new();
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
            .any(|effect| telekinesis.contains(&order.resolve(owner.file, effect.base_effect)))
        {
            associated.push(id);
            println!("SPEL {id} {:?}: {:?}", spell.editor_id, spell.data);
            for effect in &spell.effects {
                let base = order.resolve(owner.file, effect.base_effect);
                println!("  effect {base}: {:?}", effect.data);
                println!("  conditions: {:?}", effect.conditions);
            }
        }
    }
    println!(
        "survey: {effect_count} MGEF, {spell_count} SPEL, {} Telekinesis MGEF, {} associated SPEL",
        telekinesis.len(),
        associated.len()
    );
    if telekinesis.is_empty() || associated.is_empty() {
        return Err("no Telekinesis records found in the five-master corpus".into());
    }
    let mut game = Game::new(order);
    let player = game.player();
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
            Ok(ids) => println!("cast {spell}: {} applications", ids.len()),
            Err(error) => {
                let unchanged = before == shared.borrow().save_state();
                println!("cast {spell}: {error}, world unchanged={unchanged}");
                if !unchanged {
                    return Err("rejected cast changed world state".into());
                }
            }
        }
    }
    Ok(())
}

fn inspect_ability(
    order: &LoadOrder,
    ability: soe_esm::GlobalFormId,
) -> Result<(), Box<dyn std::error::Error>> {
    let owner = order.winner_ref(ability).ok_or("missing ability owner")?;
    let spell = Spell::from_record(
        order.record(owner),
        order.file(owner.file).plugin.header.is_localized(),
    )?;
    println!(
        "  equip ability {ability} {:?}: {:?}",
        spell.editor_id, spell.data
    );
    for entry in &spell.effects {
        let id = order.resolve(owner.file, entry.base_effect);
        let effect_owner = order.winner_ref(id).ok_or("missing ability effect owner")?;
        let effect = MagicEffect::from_record(
            order.record(effect_owner),
            order.file(effect_owner.file).plugin.header.is_localized(),
        )?;
        println!("    MGEF {id} {:?}: {:?}", effect.editor_id, effect.data);
        println!("    EFIT {:?}", entry.data);
    }
    Ok(())
}
