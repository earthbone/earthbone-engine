// SPDX-License-Identifier: GPL-3.0-or-later

//! Print the winning detection and stealth game settings without copying installed assets.

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
    for (id, record) in order.winners_of(Tag(*b"GMST")) {
        let Some(name) = record.editor_id() else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let keys = std::env::args().skip(1).collect::<Vec<_>>();
        let wanted = if keys.is_empty() {
            ["sneak", "stealth", "detect", "detection"]
                .iter()
                .any(|key| lower.contains(key))
        } else {
            keys.iter()
                .any(|key| lower.contains(&key.to_ascii_lowercase()))
        };
        if !wanted {
            continue;
        }
        let value = record
            .subrecords
            .iter()
            .find(|sub| sub.tag == Tag(*b"DATA"))
            .map(|sub| sub.data.clone())
            .unwrap_or_default();
        let shown = match (name.as_bytes().first(), value.get(..4)) {
            (Some(b'f'), Some(bytes)) => {
                format!("{}", f32::from_le_bytes(bytes.try_into()?))
            }
            (Some(b'i' | b'u'), Some(bytes)) => {
                format!("{}", i32::from_le_bytes(bytes.try_into()?))
            }
            (Some(b'b'), Some(bytes)) => format!("{}", u32::from_le_bytes(bytes.try_into()?)),
            _ => format!("{value:02x?}"),
        };
        println!("{id} {name} = {shown}");
    }
    Ok(())
}
