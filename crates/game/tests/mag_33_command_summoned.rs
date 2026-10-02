// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;
use soe_papyrus::asm::ScriptBuilder;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOURCE: u32 = 0x100;
const TARGET: u32 = 0x101;
const CASTER: u32 = 0x102;
const OTHER_CASTER: u32 = 0x103;
const BASE: u32 = 0x500;
const SUMMON_BASE: u32 = 0x501;
const SUMMON_EFFECT: u32 = 0x200;
const SUMMON_SPELL: u32 = 0x300;
const COMMAND_EFFECT: u32 = 0x201;
const COMMAND_SPELL: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect(form: u32, archetype: u32, associated_item: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 8, associated_item);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(T(b"MGEF"), form).sub(T(b"DATA"), data)
}

fn spell(form: u32, effect: u32, duration: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, 0);
    put(&mut data, 16, 1);
    put(&mut data, 20, 3);
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(
            T(b"EFIT"),
            [
                0.0f32.to_le_bytes(),
                0u32.to_le_bytes(),
                duration.to_le_bytes(),
            ]
            .concat(),
        )
}

fn fixture() -> Game {
    let mut summonable = vec![0; 24];
    put(&mut summonable, 0, 0x4000);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"NPC_"), SUMMON_BASE).sub(T(b"ACBS"), summonable));
    for actor in [SOURCE, TARGET, CASTER, OTHER_CASTER] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), BASE.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(effect(SUMMON_EFFECT, 18, SUMMON_BASE));
    plugin.push_top(effect(COMMAND_EFFECT, 10, 0));
    plugin.push_top(spell(SUMMON_SPELL, SUMMON_EFFECT, 30));
    plugin.push_top(spell(COMMAND_SPELL, COMMAND_EFFECT, 10));
    let mut order = LoadOrder::new();
    order.add("CommandSummoned.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [SOURCE, TARGET, CASTER, OTHER_CASTER] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game.load_script(&ScriptBuilder::new("EffectProbe", "ActiveMagicEffect").bytes())
        .unwrap();
    game.bind();
    game
}

fn cast(game: &mut Game, spell: u32, caster: u32, target: GlobalFormId) -> Vec<EffectId> {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(SOURCE),
            Some(id(caster)),
            Some(target),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    result
}

fn summoned_by(game: &Game, effect: EffectId) -> GlobalFormId {
    game.world()
        .active_effects()
        .find(|active| active.id == effect)
        .unwrap()
        .summoned
        .unwrap()
}

#[test]
fn mag_33_transfers_only_a_live_summons_runtime_owner() {
    let mut game = fixture();
    let authored_flags = game.world().npc(id(SUMMON_BASE)).unwrap().config.flags;
    let summon_effect = cast(&mut game, SUMMON_SPELL, CASTER, id(SOURCE))[0];
    let summon = summoned_by(&game, summon_effect);
    assert_eq!(
        game.world().reference_ref(summon).unwrap().owner,
        Some(id(CASTER))
    );

    let command = cast(&mut game, COMMAND_SPELL, OTHER_CASTER, summon)[0];

    let world = game.world();
    assert_eq!(
        world.reference_ref(summon).unwrap().owner,
        Some(id(OTHER_CASTER))
    );
    assert_eq!(
        world.npc(id(SUMMON_BASE)).unwrap().config.flags,
        authored_flags
    );
    assert!(world
        .active_effects()
        .any(|active| active.id == command && active.target == summon));
}

#[test]
fn mag_33_ignores_actors_without_a_live_summon_effect() {
    let mut game = fixture();
    let effects = cast(&mut game, COMMAND_SPELL, OTHER_CASTER, id(TARGET));

    assert!(effects.is_empty());
    assert_eq!(game.world().reference_ref(id(TARGET)).unwrap().owner, None);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_33_restores_runtime_owner_when_command_ends() {
    let mut game = fixture();
    let summon_effect = cast(&mut game, SUMMON_SPELL, CASTER, id(SOURCE))[0];
    let summon = summoned_by(&game, summon_effect);
    game.world_mut().reference(summon).unwrap().owner = None;
    let command = cast(&mut game, COMMAND_SPELL, OTHER_CASTER, summon)[0];
    let shared = game.shared_world();
    assert!(shared
        .borrow_mut()
        .dispel_effect(command, &mut |name, class| game
            .vm
            .create_object_of(name, class)));

    assert_eq!(game.world().reference_ref(summon).unwrap().owner, None);
    assert_eq!(
        game.world().npc(id(SUMMON_BASE)).unwrap().config.flags,
        0x4000
    );
}

#[test]
fn mag_33_saved_command_restores_prior_owner_after_expiry() {
    let mut game = fixture();
    let summon_effect = cast(&mut game, SUMMON_SPELL, CASTER, id(SOURCE))[0];
    let summon = summoned_by(&game, summon_effect);
    cast(&mut game, COMMAND_SPELL, OTHER_CASTER, summon);
    let bytes = game.save_state().unwrap();

    let mut loaded = fixture();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(
        loaded.world().reference_ref(summon).unwrap().owner,
        Some(id(OTHER_CASTER))
    );
    loaded.tick(10.0);

    assert_eq!(
        loaded.world().reference_ref(summon).unwrap().owner,
        Some(id(CASTER))
    );
}
