// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const TARGET: u32 = 0x101;
const BASE: u32 = 0x200;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;
const HAZARD: u32 = 0x400;

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn game() -> Game {
    let mut magic_effect = vec![0; 152];
    put(&mut magic_effect, 8, HAZARD);
    put(&mut magic_effect, 64, 40);
    put(&mut magic_effect, 80, 1);
    put(&mut magic_effect, 84, 3);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 3);
    let mut efit = 20.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&5u32.to_le_bytes());
    let mut hazard_data = vec![0; 40];
    put(&mut hazard_data, 0, 4);
    put(&mut hazard_data, 4, 64.0f32.to_bits());
    put(&mut hazard_data, 8, 3.0f32.to_bits());
    put(&mut hazard_data, 16, 0.25f32.to_bits());
    put(&mut hazard_data, 20, 0x0a);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    for (form, position) in [(CASTER, [0.0f32, 0.0, 0.0]), (TARGET, [12.0, 0.0, 0.0])] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), form)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    plugin.push_top(Record::new(T(b"HAZD"), HAZARD).sub(T(b"DATA"), hazard_data));
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), magic_effect));
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("SpawnHazard.esm", plugin).unwrap();
    Game::new(order)
}

fn cast(
    game: &mut Game,
) -> Result<Vec<soe_game::world::magic::EffectId>, soe_game::world::magic::MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

#[test]
fn mag_61_valid_hazard_cast_spawns_at_the_selected_target() {
    let mut game = game();

    let applied = cast(&mut game).expect("valid HAZD link should spawn a hazard");

    assert_eq!(applied.len(), 1);
    let active = game
        .world()
        .active_effects()
        .find(|effect| effect.id == applied[0])
        .cloned()
        .expect("spawn application remains owned while its hazard is active");
    assert_eq!(active.target, id(TARGET));
    assert_eq!(active.duration, Some(5.0));
}

#[test]
fn mag_61_expiry_removes_the_owned_hazard_application() {
    let mut game = game();
    let applied = cast(&mut game).expect("valid HAZD link should spawn a hazard");
    assert_eq!(game.world().active_effects().count(), 1);

    game.tick(5.0);

    assert_eq!(game.world().active_effects().count(), 0);
    assert!(applied
        .into_iter()
        .all(|id| game.world().active_effects().all(|effect| effect.id != id)));
}
