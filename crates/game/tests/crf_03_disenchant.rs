// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Disenchant eligibility, exact stack consumption and persisted effect knowledge.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, StackSource};
use soe_game::{crafting::DisenchantError, Game};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const SWORD: u32 = 0x801;
const ENCHANTMENT: u32 = 0x802;
const EFFECT_A: u32 = 0x803;
const EFFECT_B: u32 = 0x804;
const STAFF_ENCHANTMENT: u32 = 0x805;
const DISALLOWED: u32 = 0x806;
const PLAIN_SWORD: u32 = 0x807;
const LOCKED_SWORD: u32 = 0x808;
const STAFF_SWORD: u32 = 0x809;
const MALFORMED_ENCHANTMENT: u32 = 0x80a;
const MALFORMED_SWORD: u32 = 0x80b;
const INSTANCE_ENCHANTMENT: u32 = 0x80c;

fn enchantment(id: u32, kind: u32, effects: &[u32]) -> Record {
    let mut data = vec![0; 36];
    data[20..24].copy_from_slice(&kind.to_le_bytes());
    let mut record = Record::new(Tag(*b"ENCH"), id).sub(Tag(*b"ENIT"), data);
    for effect in effects {
        record = record
            .sub(Tag(*b"EFID"), effect.to_le_bytes())
            .sub(Tag(*b"EFIT"), [0; 12]);
    }
    record
}

fn setup() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin
        .push_top(Record::new(Tag(*b"WEAP"), SWORD).sub(Tag(*b"EITM"), ENCHANTMENT.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"WEAP"), PLAIN_SWORD));
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), LOCKED_SWORD)
            .sub(Tag(*b"EITM"), ENCHANTMENT.to_le_bytes())
            .sub(Tag(*b"KSIZ"), 1_u32.to_le_bytes())
            .sub(Tag(*b"KWDA"), DISALLOWED.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), STAFF_SWORD).sub(Tag(*b"EITM"), STAFF_ENCHANTMENT.to_le_bytes()),
    );
    plugin.push_top(enchantment(ENCHANTMENT, 6, &[EFFECT_A, EFFECT_B]));
    plugin.push_top(enchantment(INSTANCE_ENCHANTMENT, 6, &[EFFECT_B]));
    plugin.push_top(enchantment(STAFF_ENCHANTMENT, 12, &[EFFECT_A]));
    plugin.push_top(
        Record::new(Tag(*b"ENCH"), MALFORMED_ENCHANTMENT)
            .sub(Tag(*b"ENIT"), {
                let mut data = vec![0; 36];
                data[20..24].copy_from_slice(&6_u32.to_le_bytes());
                data
            })
            .sub(Tag(*b"EFID"), EFFECT_A.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), MALFORMED_SWORD)
            .sub(Tag(*b"EITM"), MALFORMED_ENCHANTMENT.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT_A));
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT_B));
    plugin.push_top(
        Record::new(Tag(*b"KYWD"), DISALLOWED).sub(Tag(*b"EDID"), b"MagicDisallowEnchanting\0"),
    );
    let mut order = LoadOrder::new();
    order.add("Disenchant.esm", plugin).unwrap();
    Game::new(order)
}

fn id(game: &Game, raw: u32) -> GlobalFormId {
    game.world().order().resolve(0, raw)
}

fn add_sword(game: &mut Game) {
    let (player, sword) = (id(game, PLAYER), id(game, SWORD));
    game.world_mut().add_item(player, sword, 1);
}

fn attach_enchantment(game: &mut Game) {
    let player = id(game, PLAYER);
    let sword = id(game, SWORD);
    let selected = game
        .world_mut()
        .select_item(player, sword, StackSource::Plain, 1)
        .unwrap();
    let enchantment = id(game, INSTANCE_ENCHANTMENT);
    game.world_mut()
        .update_item(
            player,
            &selected,
            vec![Extra {
                kind: 155,
                fields: vec![Field::Form(Some(enchantment)), Field::Short(0)],
            }],
        )
        .unwrap();
}

#[test]
fn crf_03_disenchant_learns_effects_and_consumes_only_selected_instance() {
    let mut game = setup();
    add_sword(&mut game);
    add_sword(&mut game);
    attach_enchantment(&mut game);
    let player = id(&game, PLAYER);
    let sword = id(&game, SWORD);
    let selected = game
        .world_mut()
        .select_item(player, sword, StackSource::Extra(0), 1)
        .unwrap();
    let result = game
        .disenchant(player, sword, StackSource::Extra(0))
        .unwrap();
    assert_eq!(result.enchantment, id(&game, INSTANCE_ENCHANTMENT));
    assert_eq!(result.learned_effects, vec![id(&game, EFFECT_B)]);
    assert_eq!(game.world_mut().item_count(player, sword), 1);
    assert!(game
        .world()
        .learned_enchantment_effects
        .contains(&id(&game, EFFECT_B)));
    assert!(!game
        .world()
        .learned_enchantment_effects
        .contains(&id(&game, EFFECT_A)));
    assert!(game
        .world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .selected_stack(&selected)
        .is_err());
}

#[test]
fn crf_03_disenchant_rejects_ineligible_items_without_mutation() {
    let mut game = setup();
    let player = id(&game, PLAYER);
    let plain_sword = id(&game, PLAIN_SWORD);
    game.world_mut().add_item(player, plain_sword, 1);
    game.world_mut().actor(player);
    let before = game.save_state().unwrap();
    assert_eq!(
        game.disenchant(player, plain_sword, StackSource::Plain),
        Err(DisenchantError::Ineligible)
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert!(game.world().pending.is_empty());
}

#[test]
fn crf_03_disenchant_rejects_staff_and_keyword_blocked_enchantments() {
    for raw_item in [STAFF_SWORD, LOCKED_SWORD] {
        let mut game = setup();
        let player = id(&game, PLAYER);
        let item = id(&game, raw_item);
        game.world_mut().add_item(player, item, 1);
        game.world_mut().actor(player);
        let before = game.save_state().unwrap();
        assert_eq!(
            game.disenchant(player, item, StackSource::Plain),
            Err(DisenchantError::Ineligible)
        );
        assert_eq!(game.save_state().unwrap(), before);
        assert!(game.world().pending.is_empty());
    }
}

#[test]
fn crf_03_disenchant_rejects_malformed_effect_groups_atomically() {
    let mut game = setup();
    let player = id(&game, PLAYER);
    let item = id(&game, MALFORMED_SWORD);
    game.world_mut().add_item(player, item, 1);
    game.world_mut().actor(player);
    let before = game.save_state().unwrap();
    assert_eq!(
        game.disenchant(player, item, StackSource::Plain),
        Err(DisenchantError::Enchantment(id(
            &game,
            MALFORMED_ENCHANTMENT
        )))
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert!(game.world().pending.is_empty());
}

#[test]
fn crf_03_disenchant_preserves_inventory_when_effects_are_already_known() {
    let mut game = setup();
    add_sword(&mut game);
    let player = id(&game, PLAYER);
    let sword = id(&game, SWORD);
    let effect_a = id(&game, EFFECT_A);
    let effect_b = id(&game, EFFECT_B);
    game.world_mut()
        .learned_enchantment_effects
        .extend([effect_a, effect_b]);
    game.world_mut().actor(player);
    let before = game.save_state().unwrap();
    assert_eq!(
        game.disenchant(player, sword, StackSource::Plain),
        Err(DisenchantError::Ineligible)
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert!(game.world().pending.is_empty());
}

#[test]
fn crf_03_disenchant_learned_effects_survive_save_load() {
    let mut game = setup();
    add_sword(&mut game);
    let player = id(&game, PLAYER);
    let sword = id(&game, SWORD);
    game.disenchant(player, sword, StackSource::Plain).unwrap();
    let expected = game.state_hash();
    let saved = game.save_state().unwrap();
    let mut restored = setup();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.state_hash(), expected);
    let effect_a = id(&restored, EFFECT_A);
    let player = id(&restored, PLAYER);
    let sword = id(&restored, SWORD);
    assert!(restored
        .world()
        .learned_enchantment_effects
        .contains(&effect_a));
    assert_eq!(restored.world_mut().item_count(player, sword), 0);
}
