// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed NPC sources from xEdit NPC_ AIDT, PKID, CNAM and ZNAM.

use soe_esm::{GlobalFormId, Tag};
use soe_game::npc::template;

#[test]
fn r4_installed_giant_sources_and_combat_style() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let game = source.localized_game().unwrap();
    let mut world = game.world_mut();
    for object in [0x33f9f, 0xc97d2, 0x1a66e] {
        let actor = GlobalFormId { file: 0, object };
        let base = world.base_of(actor).unwrap();
        let (dead, ai_disabled, race) = {
            let state = world.actor(actor).unwrap();
            (state.dead, state.ai_disabled, state.race)
        };
        let aggression = world.actor_value(actor, "aggression");
        let confidence = world.actor_value(actor, "confidence");
        println!("R4_SOURCE actor={actor} base={base} dead={dead} ai_disabled={ai_disabled} race={race:?} aggression={aggression} confidence={confidence}");
        for (name, group) in [
            ("ai", template::AI_DATA),
            ("packages", template::AI_PACKAGES),
            ("stats", template::STATS),
            ("traits", template::TRAITS),
        ] {
            let owner = world.actor_component_source(actor, group);
            println!("R4_SOURCE actor={actor} group={name} source={owner:?}");
            let Ok(owner) = owner else { continue };
            let record = world.record(owner).unwrap();
            let npc = world.npc(owner).unwrap();
            println!("R4_SOURCE actor={actor} group={name} editor={:?} data={:?} class={:?} packages={:?}", npc.editor_id, npc.ai_data, npc.class.and_then(|raw| world.resolve_from(owner, raw)).and_then(|id| world.editor_id(id)), npc.packages.iter().filter_map(|raw| world.resolve_from(owner, *raw)).map(|id| (id, world.editor_id(id))).collect::<Vec<_>>());
            if let Some(bytes) = record.find(Tag(*b"ZNAM")) {
                let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                if let Some(style) = world.resolve_from(owner, raw).filter(|_| raw != 0) {
                    let Some(record) = world.record(style) else {
                        continue;
                    };
                    let close: Vec<_> = record
                        .find(Tag(*b"CSCR"))
                        .unwrap_or_default()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|bytes| f32::from_le_bytes(*bytes))
                        .collect();
                    println!("R4_SOURCE actor={actor} combat_style={style} editor={:?} close_range={close:?}", record.editor_id());
                }
            }
        }
    }
}
