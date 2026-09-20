// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspect installed footstep records without changing game data.

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
    if std::env::args().any(|arg| arg == "--sounds") {
        return sounds(&order);
    }
    if std::env::args().any(|arg| arg == "--addons") {
        return addons(&order);
    }
    for (id, record) in order.winners_of(Tag::new(b"MATT")) {
        let name = record.find(Tag::new(b"MNAM")).map(soe_esm::cursor::zstring);
        println!("MATT {id} {:?} {name:?}", record.editor_id());
    }
    for (id, record) in order.winners_of(Tag::new(b"DOBJ")) {
        for owner in order.overrides(id) {
            if let Some(data) = order.record(*owner).find(Tag::new(b"DNAM")) {
                for pair in data
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .filter(|p| &p[..4] == b"DFTS")
                {
                    println!("DOBJ history file={} DFTS {:02x?}", owner.file, &pair[4..]);
                }
            }
        }
        if let Some(data) = record.find(Tag::new(b"DNAM")) {
            for pair in data
                .as_chunks::<8>()
                .0
                .iter()
                .filter(|p| &p[..4] == b"DFTS")
            {
                println!("DOBJ {id} DFTS {:02x?}", &pair[4..]);
            }
        }
    }
    for (id, record) in order.winners_of(Tag::new(b"FSTS")) {
        println!(
            "FSTS {id} {:?} {:?}",
            record.editor_id(),
            record.find(Tag::new(b"XCNT"))
        );
        if id.file == 0 && id.object == 0x12F16 {
            let owner = order.winner_ref(id).ok_or("no FSTS owner")?;
            for bytes in record.required(Tag::new(b"DATA"))?.as_chunks::<4>().0 {
                let raw =
                    soe_esm::cursor::Fields::new(record.tag, Tag::new(b"DATA"), bytes).u32()?;
                let id = order.resolve(owner.file, raw);
                let step = order.winner(id).ok_or("missing FSTP")?;
                println!(
                    "DEFAULT STEP {id} {:?} tag={:?}",
                    step.editor_id(),
                    step.find(Tag::new(b"ANAM")).map(soe_esm::cursor::zstring)
                );
            }
        }
    }
    Ok(())
}

fn addons(order: &LoadOrder) -> Result<(), Box<dyn std::error::Error>> {
    for (id, record) in order.winners_of(Tag::new(b"ARMA")) {
        let Some(sound) = record.find(Tag::new(b"SNDD")) else {
            continue;
        };
        let owner = order.winner_ref(id).ok_or("no addon owner")?;
        let raw = soe_esm::cursor::Fields::new(record.tag, Tag::new(b"SNDD"), sound).u32()?;
        if raw == 0 {
            continue;
        }
        let set = order.resolve(owner.file, raw);
        println!(
            "ARMA {id} {:?} slots={:?} priority={:?} FSTS={set} {:?}",
            record.editor_id(),
            record
                .find(Tag::new(b"BOD2"))
                .or_else(|| record.find(Tag::new(b"BODT"))),
            record.find(Tag::new(b"DNAM")),
            order.winner(set).and_then(soe_esm::Record::editor_id)
        );
    }
    Ok(())
}

fn sounds(order: &LoadOrder) -> Result<(), Box<dyn std::error::Error>> {
    use soe_game::footsteps::{Footsteps, Gait};
    let steps = Footsteps::new(order);
    let set = steps.default_object(*b"DFTS")?.ok_or("no DFTS")?;
    let mut ids = std::collections::BTreeSet::new();
    for hash in [500_811_281, 3_741_512_247, 3_106_094_762] {
        let material = steps.material(hash)?.ok_or("no material")?;
        for gait in [Gait::Walk, Gait::Run, Gait::Sprint, Gait::Sneak] {
            let tags = if gait == Gait::Sprint {
                ["FootSprintLeft", "FootSprintRight"]
            } else {
                ["FootLeft", "FootRight"]
            };
            for tag in tags {
                for impact in steps.impacts(set, gait, tag, material)? {
                    ids.extend(impact.sounds.into_iter().flatten());
                }
            }
        }
    }
    let mut pending: Vec<_> = ids.into_iter().collect();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let owner = order.winner_ref(id).ok_or("no sound record")?;
        let record = order.record(owner);
        println!("{} {id} {:?}", record.tag, record.editor_id());
        for sub in &record.subrecords {
            if sub.tag == Tag::new(b"EDID") {
                continue;
            }
            if sub.tag == Tag::new(b"ANAM") && record.tag == Tag::new(b"SNDR") {
                println!("  file {:?}", soe_esm::cursor::zstring(&sub.data));
            } else {
                println!("  {} {:?}", sub.tag, sub.data);
            }
            let link = if record.tag == Tag::new(b"SNDR") {
                [Tag::new(b"GNAM"), Tag::new(b"ONAM"), Tag::new(b"SNAM")].contains(&sub.tag)
            } else {
                record.tag == Tag::new(b"SNCT") && sub.tag == Tag::new(b"PNAM")
            };
            if link {
                let raw = soe_esm::cursor::Fields::new(record.tag, sub.tag, &sub.data).u32()?;
                if raw != 0 {
                    pending.push(order.resolve(owner.file, raw));
                }
            }
        }
    }
    Ok(())
}
