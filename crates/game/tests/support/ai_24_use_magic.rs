// SPDX-License-Identifier: GPL-3.0-or-later
// AI-24 UseMagic procedure execution through the shared charged magic path.

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACKAGE: u32 = 0x801;
const BASE: u32 = 0x802;
const CASTER: u32 = 0x803;
const TARGET: u32 = 0x804;
const EFFECT: u32 = 0x805;
const SPELL: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(target_kind: u32, spell_type: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });

    let template = package::with_input_name(
        package::with_procedure(
            package::finish(package::close_header(
                package::build_template(TEMPLATE, "UseMagicTemplate"),
                None,
            )),
            "UseMagic",
            &[0],
            true,
        ),
        0,
        "Target",
        true,
    );
    plugin.push_top(template);

    let mut pack = package::close_header(
        package::build_record(PACKAGE, "UseMagicPackage", 0, Schedule::default()),
        Some(TEMPLATE),
    );
    let mut target_data = target_kind.to_le_bytes().to_vec();
    target_data.extend_from_slice(&TARGET.to_le_bytes());
    target_data.extend_from_slice(&0i32.to_le_bytes());
    pack = pack
        .zsub(T(b"ANAM"), "Target")
        .sub(T(b"PTDA"), target_data)
        .sub(T(b"UNAM"), [0u8]);
    plugin.push_top(package::with_actions(package::finish(pack)));

    let base = npc::build_record(BASE, "Mage", "Mage", Config::default(), Stats::default())
        .sub(T(b"PKID"), PACKAGE.to_le_bytes());
    plugin.push_top(base);
    plugin.push_top(
        Record::new(T(b"ACHR"), CASTER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), TARGET)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );

    let mut effect_data = vec![0; 152];
    effect_data[0..4].copy_from_slice(&(1u32 << 1).to_le_bytes());
    effect_data[4..8].copy_from_slice(&7.0f32.to_le_bytes());
    effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect_data[64..68].copy_from_slice(&0u32.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));

    let mut spell_data = vec![0; 36];
    spell_data[0..4].copy_from_slice(&7u32.to_le_bytes());
    spell_data[4..8].copy_from_slice(&1u32.to_le_bytes());
    spell_data[8..12].copy_from_slice(&spell_type.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [5.0f32.to_le_bytes(), 0u32.to_le_bytes(), 0u32.to_le_bytes()].concat(),
            ),
    );

    let mut order = LoadOrder::new();
    order.add("Ai24.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().drive_actor(id(CASTER));
    game.world_mut().drive_actor(id(TARGET));
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 20.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(TARGET), "health", 100.0)
        .unwrap();
    game.world_mut()
        .actor(id(CASTER))
        .unwrap()
        .spells
        .insert(id(SPELL));
    if spell_type == 0 {
        game.world_mut()
            .equip_hand_spell(id(CASTER), Hand::Right, id(SPELL))
            .unwrap();
    } else {
        game.world_mut().actor(id(CASTER)).unwrap().right_spell = Some(id(SPELL));
    }
    game
}

#[test]
fn ai_24_use_magic_casts_assigned_spell_at_resolved_target_and_pays_cost() {
    let mut game = fixture(0, 0);

    game.tick(0.1);

    let ai = game.world().actor_ref(id(CASTER)).unwrap().ai.clone();
    assert_eq!(
        game.world_mut().actor_value(id(TARGET), "health"),
        105.0,
        "actor state: {ai:?}"
    );
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 13.0);
    assert!(
        game.world()
            .actor_ref(id(CASTER))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
    assert!(game
        .world()
        .actor_ref(id(CASTER))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(PACKAGE)));
}

#[test]
fn ai_24_use_magic_rejects_unresolved_target_without_spending_magicka() {
    let mut game = fixture(7, 0);

    game.tick(0.1);

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 20.0);
}

#[test]
fn ai_24_use_magic_rejects_insufficient_magicka_without_applying_effects() {
    let mut game = fixture(0, 0);
    game.world_mut()
        .set_actor_value(id(CASTER), "magicka", 2.0)
        .unwrap();

    game.tick(0.1);

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 2.0);
}

#[test]
fn ai_24_use_magic_rejects_an_unusable_assigned_spell_without_spending_magicka() {
    let mut game = fixture(0, 1);

    game.tick(0.1);

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 100.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 20.0);
}

#[test]
fn prf_27_spell_choice_timing_wraps_an_actual_usemagic_cast() {
    let mut game = fixture(0, 0);
    game.tick(0.1);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 105.0);
    assert!(game.world().ai.planning_phases["spell_choice"] > std::time::Duration::ZERO);
    assert!(game.world().ai.planning_phases["procedure.UseMagic"] > std::time::Duration::ZERO);
}
