// SPDX-License-Identifier: GPL-3.0-or-later

//! Inspect the published character-controller fields across installed packfiles.

use std::collections::BTreeMap;
use std::path::PathBuf;

use soe_bsa::Archive;
use soe_hkx::character::CharacterData;
use soe_hkx::Packfile;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(std::env::var("SKYRIM_DATA")?);
    let archive = Archive::open(&data.join("Skyrim - Animations.bsa"))?;
    let mut groups = BTreeMap::new();
    let mut total = 0;
    for path in archive.files().iter().filter(|path| {
        std::path::Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("hkx"))
    }) {
        let bytes = archive.read(path)?;
        let pack = Packfile::parse(&bytes)?;
        for object in pack.objects_of("hkbCharacterData") {
            let character = CharacterData::from_obj(object)?;
            let height = object.f32(16)?;
            let radius = object.f32(20)?;
            let filter = object.u32(24)?;
            let cinfo = object.ptr(32)?.and_then(|o| o.class());
            println!(
                "{path}: height={height} radius={radius} filter={filter:#x} cinfo={cinfo:?} scale={} rig={}",
                character.scale, character.rig_name
            );
            *groups
                .entry((height.to_bits(), radius.to_bits(), filter))
                .or_insert(0) += 1;
            total += 1;
        }
    }
    println!(
        "Character files={total}, geometry/filter groups={}",
        groups.len()
    );
    for ((height, radius, filter), count) in groups {
        println!(
            "{count}: height={} radius={} filter={filter:#x}",
            f32::from_bits(height),
            f32::from_bits(radius)
        );
    }
    Ok(())
}
