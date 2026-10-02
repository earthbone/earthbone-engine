// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::magic::EffectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ANCHOR: u32 = 0x100;
const NEAR: u32 = 0x101;
const FAR: u32 = 0x102;
const BASE: u32 = 0x500;
const CLOAK: u32 = 0x200;
const PULSE: u32 = 0x201;
const CLOAK_SPELL: u32 = 0x300;
const PULSE_SPELL: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn mgef(archetype: u32, associated: u32, area: u32, av: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 8, associated);
    put(&mut data, 44, area);
    put(&mut data, 64, archetype);
    put(&mut data, 68, av);
    put(&mut data, 80, if archetype == 35 { 1 } else { 2 });
    put(&mut data, 84, if archetype == 35 { 0 } else { 2 });
    Record::new(T(b"MGEF"), if archetype == 35 { CLOAK } else { PULSE }).sub(T(b"DATA"), data)
}

fn spell(
    form: u32,
    effect: u32,
    magnitude: f32,
    duration: u32,
    cast_type: u32,
    delivery: u32,
) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, 0);
    put(&mut data, 16, cast_type);
    put(&mut data, 20, delivery);
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(
            T(b"EFIT"),
            [
                magnitude.to_le_bytes(),
                0u32.to_le_bytes(),
                duration.to_le_bytes(),
            ]
            .concat(),
        )
}

fn fixture() -> Game {
    fixture_with_cloak_duration(2)
}

fn fixture_with_cloak_duration(cloak_duration: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for (actor, x) in [(ANCHOR, 0.0f32), (NEAR, 5.0), (FAR, 50.0)] {
        let mut placement = vec![0; 24];
        placement[0..4].copy_from_slice(&x.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), placement),
        );
    }
    plugin.push_top(mgef(35, PULSE_SPELL, 1, u32::MAX));
    plugin.push_top(mgef(0, 0, 0, 24));
    plugin.push_top(spell(CLOAK_SPELL, CLOAK, 10.0, cloak_duration, 1, 0));
    plugin.push_top(spell(PULSE_SPELL, PULSE, 0.0, 10, 2, 2));

    let mut order = LoadOrder::new();
    order.add("Mag56.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ANCHOR, NEAR, FAR] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut().reference(id(actor)).unwrap();
    }
    game
}

fn cast_cloak(game: &mut Game) -> EffectId {
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(CLOAK_SPELL),
            id(ANCHOR),
            Some(id(ANCHOR)),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    applied[0]
}

fn targets(game: &Game) -> Vec<GlobalFormId> {
    let mut targets: Vec<_> = game
        .world()
        .active_effects()
        .filter(|effect| effect.effect == id(PULSE))
        .map(|effect| effect.target)
        .collect();
    targets.sort();
    targets
}

#[test]
fn mag_56_cloak_pulses_on_its_one_second_cadence_and_selects_nearby_actors() {
    let mut game = fixture();
    cast_cloak(&mut game);

    game.tick(0.75);
    assert!(
        targets(&game).is_empty(),
        "cloak must wait for its pulse interval"
    );
    game.tick(0.25);

    let selected = targets(&game);
    assert!(selected.contains(&id(ANCHOR)) && selected.contains(&id(NEAR)));
    assert!(!selected.contains(&id(FAR)));
}

#[test]
fn mag_56_cloak_expiry_cleans_up_only_its_owned_effects_and_survives_save_load() {
    let mut game = fixture();
    cast_cloak(&mut game);
    game.tick(1.0);
    let selected = targets(&game);
    assert!(selected.contains(&id(ANCHOR)) && selected.contains(&id(NEAR)));
    assert!(!selected.contains(&id(FAR)));

    let bytes = game.world().save_state();
    let mut restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    restored.tick(1.0);

    assert!(
        targets(&restored).is_empty(),
        "expired cloak must remove its child effects"
    );
    assert_eq!(restored.world().active_effects().count(), 0);
}

#[test]
fn mag_56_cloak_catches_up_each_full_cadence_interval() {
    let mut game = fixture_with_cloak_duration(10);
    cast_cloak(&mut game);
    game.tick(2.0);

    let mut ids: Vec<_> = game
        .world()
        .active_effects()
        .filter(|effect| effect.effect == id(PULSE))
        .map(|effect| effect.id.0)
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![4, 5], "two cadence pulses replace the first set");
}
