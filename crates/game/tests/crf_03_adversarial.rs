// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for CRF-03 disenchant eligibility and atomicity.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::StackSource;
use soe_game::{Game, crafting::DisenchantError};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const ITEM: u32 = 0x801;
const ENCHANT: u32 = 0x802;
const EFFECT: u32 = 0x803;
const MISSING_EFFECT: u32 = 0x804;

fn id(game: &Game, object: u32) -> GlobalFormId {
    game.world().order().resolve(0, object)
}

fn enit(len: usize) -> Vec<u8> {
    let mut data = vec![0; len];
    data[20..24].copy_from_slice(&6_u32.to_le_bytes());
    data
}

fn game(effect: u32, enit_len: usize, include_effect: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"WEAP"), ITEM).sub(Tag(*b"EITM"), ENCHANT.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"ENCH"), ENCHANT)
            .sub(Tag(*b"ENIT"), enit(enit_len))
            .sub(Tag(*b"EFID"), effect.to_le_bytes())
            .sub(Tag(*b"EFIT"), [0; 12]),
    );
    if include_effect {
        plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT));
    }
    let mut order = LoadOrder::new();
    order.add("Crf03Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn crf_03_accepts_enit_without_optional_trailing_worn_restrictions() {
    // UESP documents 32-byte ENIT records in form version 37, which omit the
    // final form ID, so the enchantment kind and effects remain readable.
    let mut game = game(EFFECT, 32, true);
    let player = id(&game, PLAYER);
    let item = id(&game, ITEM);
    game.world_mut().add_item(player, item, 1);

    let result = game.disenchant(player, item, StackSource::Plain);
    assert!(
        result.is_ok(),
        "valid optional-tail ENIT was rejected: {result:?}"
    );
    assert!(
        game.world()
            .learned_enchantment_effects
            .contains(&id(&game, EFFECT))
    );
    assert_eq!(game.world_mut().item_count(player, item), 0);
}

#[test]
fn crf_03_rejects_other_enit_lengths_without_mutation() {
    let mut game = game(EFFECT, 33, true);
    let player = id(&game, PLAYER);
    let item = id(&game, ITEM);
    game.world_mut().add_item(player, item, 1);
    game.world_mut().actor(player);
    let before = game.save_state().unwrap();

    assert_eq!(
        game.disenchant(player, item, StackSource::Plain),
        Err(DisenchantError::Enchantment(id(&game, ENCHANT)))
    );
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn crf_03_missing_magic_effect_record_rejects_without_consuming_or_learning() {
    let mut game = game(MISSING_EFFECT, 36, false);
    let player = id(&game, PLAYER);
    let item = id(&game, ITEM);
    game.world_mut().add_item(player, item, 1);
    game.world_mut().actor(player);
    let before = game.save_state().unwrap();

    assert_eq!(
        game.disenchant(player, item, StackSource::Plain),
        Err(DisenchantError::Enchantment(id(&game, ENCHANT)))
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert!(game.world().learned_enchantment_effects.is_empty());
    assert!(game.world().pending.is_empty());
}

#[test]
fn crf_03_second_call_does_not_consume_another_identical_item() {
    let mut game = game(EFFECT, 36, true);
    let player = id(&game, PLAYER);
    let item = id(&game, ITEM);
    game.world_mut().add_item(player, item, 2);

    assert!(game.disenchant(player, item, StackSource::Plain).is_ok());
    assert_eq!(game.world_mut().item_count(player, item), 1);
    let before = game.save_state().unwrap();
    assert_eq!(
        game.disenchant(player, item, StackSource::Plain),
        Err(DisenchantError::Ineligible)
    );
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(game.world_mut().item_count(player, item), 1);
}
