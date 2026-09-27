// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::MagicError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn condition(function: u16, parameter: u32, expected: f32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data
}

fn fixture(cast_type: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"NPC_"), 0x501).sub(T(b"ACBS"), vec![0; 24]));
    for (form, position) in [
        (0x100, [0.0_f32, 0.0, 0.0]),
        (0x101, [100.0, 0.0, 0.0]),
        (0x102, [200.0, 0.0, 0.0]),
        (0x103, [200.0, 0.0, 0.0]),
        (0x104, [600.0, 0.0, 0.0]),
    ] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), form)
                .sub(
                    T(b"NAME"),
                    (if form == 0x102 { 0x501u32 } else { 0x500u32 }).to_le_bytes(),
                )
                .sub(T(b"DATA"), data),
        );
    }
    let mut data = vec![0; 152];
    put(&mut data, 32, 0x400);
    put(&mut data, 64, 19);
    put(&mut data, 80, cast_type);
    plugin.push_top(
        Record::new(T(b"MGEF"), 0x200)
            .sub(T(b"DATA"), data)
            .sub(T(b"CTDA"), condition(136, 0x101, 1.0)),
    );
    plugin.push_top(Record::new(T(b"EFSH"), 0x400));
    let mut spit = vec![0; 36];
    put(&mut spit, 16, cast_type);
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x300)
            .sub(T(b"SPIT"), spit.split_off(0))
            .sub(T(b"EFID"), 0x200u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    500.0f32.to_le_bytes().to_vec(),
                    0u32.to_le_bytes().to_vec(),
                    5u32.to_le_bytes().to_vec(),
                ]
                .concat(),
            ),
    );

    let mut order = LoadOrder::new();
    order.add("DetectLife.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x100, 0x101, 0x102, 0x103, 0x104] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut().reference(id(actor)).unwrap();
    }
    game.world_mut().actor(id(0x103)).unwrap().dead = true;
    game
}

fn cast_detection(game: &mut Game) -> Result<(), MagicError> {
    let shared = game.shared_world();
    shared.borrow_mut().cast_spell(
        id(0x300),
        id(0x100),
        Some(id(0x100)),
        None,
        &mut |name, class| game.vm.create_object_of(name, class),
    )?;
    Ok(())
}

#[test]
fn mag_40_active_detect_life_selects_only_live_actors_in_range_with_authored_shader() {
    let mut game = fixture(2);
    cast_detection(&mut game).unwrap();
    let targets = game.world_mut().detect_life_targets().unwrap();
    assert_eq!(
        targets.len(),
        1,
        "active effects: {:?}",
        game.world().active_effects().collect::<Vec<_>>()
    );
    assert_eq!(targets[0].actor, id(0x101));
    assert_eq!(targets[0].position, [100.0, 0.0, 0.0]);
    assert_eq!(targets[0].hit_shader, Some(id(0x400)));
}

#[test]
fn mag_40_detect_life_has_no_presentation_after_its_active_effect_ends() {
    let mut game = fixture(2);
    cast_detection(&mut game).unwrap();
    let effect = game.world().active_effects().next().unwrap().id;
    let shared = game.shared_world();
    assert!(shared
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| game
            .vm
            .create_object_of(name, class),));
    assert!(game.world_mut().detect_life_targets().unwrap().is_empty());
}

#[test]
fn mag_40_detect_life_concentration_stays_active_until_dispelled() {
    let mut game = fixture(2);
    cast_detection(&mut game).unwrap();
    assert_eq!(game.world().active_effects().count(), 1);
    assert_eq!(game.world_mut().detect_life_targets().unwrap().len(), 1);
}

#[test]
fn mag_40_vanilla_detect_life_effects_link_authored_hit_shaders() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-40 vanilla record check");
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
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut detection_effects = Vec::new();
    let mut linked_shaders = 0;
    for (form, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(form).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect =
            soe_esm::records::gameplay::MagicEffect::from_record(record, localized).unwrap();
        if effect.data.archetype != 19 {
            continue;
        }
        let data = record.find(Tag::new(b"DATA")).unwrap();
        assert!(data.len() >= 36, "{form:?} has truncated MGEF DATA");
        let shader = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);
        if shader != 0 {
            linked_shaders += 1;
            let resolved = order.resolve(winner.file, shader);
            let shader_record = order
                .winner_ref(resolved)
                .map(|reference| order.record(reference))
                .unwrap_or_else(|| panic!("{form:?} points to a missing shader"));
            assert_eq!(shader_record.tag, Tag::new(b"EFSH"), "{form:?} shader type");
        }
        detection_effects.push((form, effect.editor_id, shader));
    }
    assert!(
        !detection_effects.is_empty(),
        "installed masters should contain Detect Life MGEFs"
    );
    assert!(
        linked_shaders > 0,
        "installed Detect Life MGEFs should link at least one authored shader"
    );
}
