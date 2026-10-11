// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOURCE: u32 = 0x100;
const TARGET: u32 = 0x101;
const CASTER: u32 = 0x102;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const BASE: u32 = 0x500;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(duration: u32) -> Game {
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 8, u32::MAX);
    put(&mut effect_data, 64, 45);
    put(&mut effect_data, 80, 2);
    put(&mut effect_data, 84, 0);

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    put(&mut spell_data, 16, 2);
    put(&mut spell_data, 20, 0);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [SOURCE, TARGET, CASTER] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    0.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    duration.to_le_bytes(),
                ]
                .concat(),
            ),
    );

    let mut order = LoadOrder::new();
    order.add("GrabActor.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [SOURCE, TARGET, CASTER] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(game: &mut Game) -> EffectId {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .cast_grab_actor_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(CASTER)),
            id(TARGET),
            &mut |_, _| {
                game.vm
                    .create_object_of("GrabActorEffect", "ActiveMagicEffect")
            },
        )
        .unwrap();
    result[0]
}

#[test]
fn mag_66_grab_actor_controls_the_selected_actor_until_dispelled() {
    let mut game = fixture(30);
    let effect = cast(&mut game);

    let world = game.world();
    let controlled = world.grab_actor_effect_on(id(TARGET)).unwrap();
    assert_eq!(controlled.id, effect);
    assert_eq!(controlled.caster, Some(id(CASTER)));
    assert_eq!(controlled.target, id(TARGET));
    drop(world);

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    assert!(game.world().grab_actor_effect_on(id(TARGET)).is_none());
}

#[test]
fn mag_66_grab_actor_releases_controlled_actor_on_expiry() {
    let mut game = fixture(2);
    cast(&mut game);

    assert!(game.world().grab_actor_effect_on(id(TARGET)).is_some());
    game.tick(2.0);

    assert!(game.world().grab_actor_effect_on(id(TARGET)).is_none());
}

#[test]
fn mag_66_grab_actor_releases_controlled_actor_on_death() {
    let mut game = fixture(30);
    cast(&mut game);
    game.world_mut().actor(id(TARGET)).unwrap().dead = true;

    game.tick(0.1);

    assert!(game.world().grab_actor_effect_on(id(TARGET)).is_none());
}

#[test]
fn mag_66_host_actor_pose_updates_the_existing_reference_placement() {
    let game = fixture(30);
    let mut world = game.world_mut();
    assert!(world.set_host_actor_pose(id(TARGET), [12.0, 34.0, 56.0], [0.1, 0.2, 0.3]));
    let placement = world.ref_placement(id(TARGET)).unwrap();
    assert_eq!(placement.position, [12.0, 34.0, 56.0]);
    assert_eq!(placement.rotation, [0.1, 0.2, 0.3]);
}

#[test]
fn mag_66_host_actor_pose_rejects_invalid_or_non_actor_references() {
    let game = fixture(30);
    let mut world = game.world_mut();
    assert!(!world.set_host_actor_pose(id(SPELL), [1.0; 3], [0.0; 3]));
    assert!(!world.set_host_actor_pose(id(TARGET), [f32::NAN, 0.0, 0.0], [0.0; 3]));
    assert_eq!(world.ref_placement(id(TARGET)).unwrap().position, [0.0; 3]);
}
