// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::CallMethod;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EFFECT: u32 = 0x800;
const ART: u32 = 0x801;
const SHADER: u32 = 0x802;
const TARGET: u32 = 0x900;
const OTHER_TARGET: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"ARTO"), ART).zsub(T(b"MODL"), "Effects/Spark.nif"));
    plugin.push_top(Record::new(T(b"EFSH"), SHADER));
    let mut data = Vec::new();
    data.extend_from_slice(&ART.to_le_bytes());
    data.extend_from_slice(&SHADER.to_le_bytes());
    data.extend_from_slice(&5_u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"RFCT"), EFFECT).sub(T(b"DATA"), data));
    for target in [TARGET, OTHER_TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), target));
    }
    let mut order = LoadOrder::new();
    order.add("VisualEffects.esm", plugin).unwrap();
    Game::new(order)
}

fn call_script(game: &mut Game) {
    let script = ScriptBuilder::new("EffectCalls", "")
        .function(
            FunctionBuilder::new("play", "None")
                .global()
                .param("effect", "VisualEffect")
                .param("target", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![s("Play"), ident("effect"), ident("unused"), ident("target")],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("stop", "None")
                .global()
                .param("effect", "VisualEffect")
                .param("target", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![s("Stop"), ident("effect"), ident("unused"), ident("target")],
                )
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
}

fn call(game: &mut Game, name: &str, target: u32) {
    let effect = game.object_of_form(id(EFFECT));
    let target = game.object_of_form(id(target));
    game.vm
        .call_global(
            "EffectCalls",
            name,
            &[Value::Object(effect), Value::Object(target)],
        )
        .unwrap();
    assert!(game.run_until_idle(32));
}

#[test]
fn mag_24_visual_effect_play_attaches_authored_art_and_stop_releases_target() {
    let mut game = fixture();
    call_script(&mut game);

    call(&mut game, "play", TARGET);
    let attachment = game.world().visual_effects().next().copied().unwrap();
    assert_eq!(attachment.effect, id(EFFECT));
    assert_eq!(attachment.target, id(TARGET));
    assert_eq!(attachment.art, Some(id(ART)));
    assert_eq!(attachment.shader, Some(id(SHADER)));
    assert_eq!(attachment.flags, 5);

    call(&mut game, "play", OTHER_TARGET);
    assert_eq!(game.world().visual_effects().count(), 2);
    call(&mut game, "stop", TARGET);
    let remaining: Vec<_> = game.world().visual_effects().copied().collect();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].target, id(OTHER_TARGET));
    call(&mut game, "stop", OTHER_TARGET);
    assert_eq!(game.world().visual_effects().count(), 0);
}

#[test]
fn mag_24_visual_effect_rejects_wrong_records_and_targets_without_partial_attachment() {
    let game = fixture();
    assert!(!game.world_mut().attach_visual_effect(id(ART), id(TARGET)));
    assert!(
        !game
            .world_mut()
            .attach_visual_effect(id(EFFECT), id(SHADER))
    );
    assert!(game.world().visual_effects().next().is_none());
}

#[test]
fn mag_24_visual_effect_reads_installed_vanilla_art_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping vanilla VisualEffect survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let game = Game::new(order);
    let authored: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"RFCT"))
        .filter_map(|(id, record)| {
            (!record.is_deleted())
                .then(|| game.world().visual_effect_data(id))
                .flatten()
                .filter(|visual| visual.art.is_some() || visual.shader.is_some())
                .map(|visual| (id, visual))
        })
        .collect();
    assert!(
        !authored.is_empty(),
        "vanilla must contain authored visual effects"
    );
    assert!(authored.iter().any(|(_, visual)| visual.art.is_some()));
    assert!(authored.iter().any(|(_, visual)| visual.shader.is_some()));
}
