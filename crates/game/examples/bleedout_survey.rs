// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspect installed bleedout inputs without changing game data.
//! CLAS DATA offset 24 follows xEdit's Bleedout Default and UESP's CLAS layout.

use std::collections::BTreeMap;

use soe_esm::{cursor::Fields, LoadOrder, Plugin, Tag};
use soe_game::npc::{flags, Npc};

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
    for (id, record) in order.winners_of(Tag(*b"GMST")) {
        let Some(name) = record.editor_id() else {
            continue;
        };
        if !name.starts_with('f')
            || !(name.to_ascii_lowercase().contains("bleedout")
                || name.to_ascii_lowercase().contains("essential"))
        {
            continue;
        }
        let tag = Tag(*b"DATA");
        let mut fields = Fields::new(record.tag, tag, record.required(tag)?);
        fields.exact(4)?;
        println!("GMST {id} {name} {}", fields.f32()?);
    }
    let mut classes = BTreeMap::<u32, usize>::new();
    for (_, record) in order.winners_of(Tag(*b"CLAS")) {
        let tag = Tag(*b"DATA");
        let mut fields = Fields::new(record.tag, tag, record.required(tag)?);
        fields.exact(36)?;
        fields.skip(24)?;
        let threshold = fields.f32()?;
        if !threshold.is_finite() {
            return Err("non-finite class bleedout threshold".into());
        }
        *classes.entry(threshold.to_bits()).or_default() += 1;
    }
    for (bits, count) in classes {
        println!("CLAS threshold={} count={count}", f32::from_bits(bits));
    }
    let mut overrides = BTreeMap::<u16, usize>::new();
    let mut selected = 0;
    let mut inherited = 0;
    for (id, record) in order.winners_of(Tag(*b"NPC_")) {
        let owner = order.winner_ref(id).ok_or("missing NPC owner")?;
        let localized = order.files()[owner.file].plugin.header.is_localized();
        let npc = Npc::from_record(record, localized)?;
        if npc.config.flags & flags::BLEEDOUT_OVERRIDE != 0 {
            *overrides.entry(npc.config.bleedout).or_default() += 1;
        }
        if npc.config.flags & (flags::ESSENTIAL | flags::PROTECTED) != 0 {
            selected += 1;
            if npc.uses_template(soe_game::npc::template::STATS) {
                inherited += 1;
            }
            println!(
                "NPC {id} {:?} flags={:08x} override={} class={:?} template={:?}",
                npc.editor_id, npc.config.flags, npc.config.bleedout, npc.class, npc.template
            );
        }
    }
    for (value, count) in overrides {
        println!("NPC bleedout_override={value} count={count}");
    }
    println!("NPC direct essential/protected={selected} inherited_stats={inherited}");
    Ok(())
}
