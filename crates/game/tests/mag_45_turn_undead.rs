// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x100;
const HUMAN: u32 = 0x101;
const UNDEAD: u32 = 0x102;
const ALLY: u32 = 0x103;
const HUMAN_BASE: u32 = 0x200;
const UNDEAD_BASE: u32 = 0x201;
const UNDEAD_KEYWORD: u32 = 0x202;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn actor(form: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = vec![0; 24];
    for (axis, value) in position.into_iter().enumerate() {
        data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(T(b"ACHR"), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), HUMAN_BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"NPC_"), UNDEAD_BASE)
            .sub(T(b"ACBS"), vec![0; 24])
            .sub(T(b"KWDA"), UNDEAD_KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"KYWD"), UNDEAD_KEYWORD).zsub(T(b"EDID"), "ActorTypeUndead"));
    plugin.push_top(actor(CASTER, HUMAN_BASE, [0.0, 0.0, 0.0]));
    plugin.push_top(actor(HUMAN, HUMAN_BASE, [5.0, 0.0, 0.0]));
    plugin.push_top(actor(UNDEAD, UNDEAD_BASE, [5.0, 0.0, 0.0]));
    plugin.push_top(actor(ALLY, UNDEAD_BASE, [5.0, 5.0, 0.0]));

    let mut effect = vec![0; 152];
    put(&mut effect, 0, 1);
    put(&mut effect, 16, u32::MAX);
    put(&mut effect, 64, 24);
    put(&mut effect, 68, 24);
    put(&mut effect, 80, 0);
    put(&mut effect, 84, 3);
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect));

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 0);
    put(&mut spell_data, 20, 3);
    let mut entry = 10.0_f32.to_le_bytes().to_vec();
    entry.extend_from_slice(&0_u32.to_le_bytes());
    entry.extend_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), entry),
    );

    let mut order = LoadOrder::new();
    order.add("TurnUndead.esm", plugin).unwrap();
    let game = Game::new(order);
    for form in [CASTER, HUMAN, UNDEAD, ALLY] {
        game.world_mut()
            .set_actor_value(id(form), "health", 100.0)
            .unwrap();
    }
    game
}

fn cast(game: &mut Game, target: u32) -> Vec<EffectId> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(target)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result.unwrap()
}

#[test]
fn mag_45_turn_undead_flees_only_for_the_authored_duration() {
    let mut game = fixture();
    let ids = cast(&mut game, UNDEAD);
    assert_eq!(ids.len(), 1);
    let world = game.world();
    let active = world.active_effects().next().unwrap();
    assert_eq!(active.duration, Some(2.0));
    assert_eq!(active.target, id(UNDEAD));
    drop(world);

    game.tick(0.1);
    let position = game.world_mut().reference(id(UNDEAD)).unwrap().position;
    assert!(
        position[0] > 5.0,
        "undead did not flee from caster: {position:?}"
    );

    game.tick(2.0);
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(
        game.world_mut().reference(id(UNDEAD)).unwrap().position,
        position
    );
}

#[test]
fn mag_45_non_undead_is_unaffected_by_turn_undead() {
    let mut game = fixture();
    let before = game.world_mut().reference(id(HUMAN)).unwrap().position;
    assert!(cast(&mut game, HUMAN).is_empty());
    assert_eq!(game.world().active_effects().count(), 0);

    game.tick(0.1);
    assert_eq!(
        game.world_mut().reference(id(HUMAN)).unwrap().position,
        before
    );
}

#[test]
fn mag_45_teammate_waits_until_combat_before_fleeing() {
    let mut game = fixture();
    game.world_mut().actor(id(ALLY)).unwrap().teammate = true;
    cast(&mut game, ALLY);
    let before = game.world_mut().reference(id(ALLY)).unwrap().position;
    game.tick(0.1);
    assert_eq!(
        game.world_mut().reference(id(ALLY)).unwrap().position,
        before
    );

    game.world_mut().actor(id(ALLY)).unwrap().in_combat = true;
    game.tick(0.1);
    let after = game.world_mut().reference(id(ALLY)).unwrap().position;
    assert!(
        after[0] > before[0],
        "teammate did not flee in combat: {after:?}"
    );
}

#[test]
fn mag_45_installed_turn_undead_records_resolve_the_required_keyword() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-45 installed-record check");
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
    let undead = order
        .winners_of(T(b"KYWD"))
        .find(|(_, record)| record.editor_id().as_deref() == Some("ActorTypeUndead"))
        .map(|(id, _)| id)
        .expect("vanilla ActorTypeUndead keyword is missing");
    let effects: Vec<_> = order
        .winners_of(T(b"MGEF"))
        .filter(|(_, record)| !record.is_deleted())
        .filter_map(|(id, record)| {
            let winner = order.winner_ref(id)?;
            let effect = soe_esm::records::gameplay::MagicEffect::from_record(
                record,
                order.file(winner.file).plugin.header.is_localized(),
            )
            .ok()?;
            (effect.data.archetype == 24).then_some(id)
        })
        .collect();
    assert!(!effects.is_empty(), "vanilla has no Turn Undead MGEF");
    assert!(
        undead.object != 0,
        "vanilla undead eligibility keyword must be a real form"
    );
    assert!(effects.iter().all(|id| order.winner_ref(*id).is_some()));
}
