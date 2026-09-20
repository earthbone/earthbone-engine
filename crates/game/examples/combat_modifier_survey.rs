// SPDX-License-Identifier: GPL-3.0-or-later

//! Print the winning skill-modifier perks without copying installed assets.

use soe_esm::{LoadOrder, Plugin, Tag};

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
    for (id, record) in order.winners_of(Tag(*b"PERK")) {
        if !matches!(
            record.editor_id().as_deref(),
            Some("PerkSkillBoosts" | "AlchemySkillBoosts")
        ) {
            continue;
        }
        println!("PERK {id} {:?}", record.editor_id());
        for sub in &record.subrecords {
            if matches!(
                &sub.tag.0,
                b"PRKE" | b"DATA" | b"PRKC" | b"CTDA" | b"EPFT" | b"EPFD" | b"PRKF" | b"PRCZ"
            ) {
                println!("  {} {:02x?}", sub.tag, sub.data);
            }
        }
    }
    Ok(())
}
