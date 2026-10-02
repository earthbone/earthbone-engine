// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Soul Trap follows the MGEF archetype table, CK TrapSoul result contract,
//! SLGM capacity fields and actor level soul-size settings.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const CASTER_BASE: u32 = 0x100;
const TARGET_BASE: u32 = 0x101;
const CASTER: u32 = 0x110;
const TARGET: u32 = 0x111;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const EMPTY_SMALL: u32 = 0x400;
const FILLED_SMALL: u32 = 0x401;
const EMPTY_COMMON: u32 = 0x402;
const FILLED_COMMON: u32 = 0x403;
const EMPTY_GRAND: u32 = 0x404;
const FILLED_GRAND: u32 = 0x405;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn soul_gem(plugin: &mut Plugin, empty: u32, filled: u32, capacity: u8, current: u8) {
    plugin.push_top(
        Record::new(Tag::new(b"SLGM"), empty)
            .sub(Tag::new(b"SOUL"), [current])
            .sub(Tag::new(b"SLCP"), [capacity])
            .sub(Tag::new(b"NAM0"), filled.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag::new(b"SLGM"), filled)
            .sub(Tag::new(b"SOUL"), [capacity])
            .sub(Tag::new(b"SLCP"), [capacity]),
    );
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [CASTER_BASE, TARGET_BASE] {
        plugin.push_top(Record::new(Tag::new(b"NPC_"), base).sub(Tag::new(b"ACBS"), vec![0; 24]));
    }
    for (reference, base) in [(CASTER, CASTER_BASE), (TARGET, TARGET_BASE)] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), reference)
                .sub(Tag::new(b"NAME"), base.to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    let mut effect_data = vec![0; 152];
    effect_data[64..68].copy_from_slice(&23u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&0u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), EFFECT).sub(Tag::new(b"DATA"), effect_data));
    plugin.push_top(
        Record::new(Tag::new(b"SPEL"), SPELL)
            .sub(
                Tag::new(b"SPIT"),
                [0u8; 36]
                    .iter()
                    .enumerate()
                    .map(|(i, b)| if i == 20 { 3 } else { *b })
                    .collect::<Vec<_>>(),
            )
            .sub(Tag::new(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                Tag::new(b"EFIT"),
                [
                    1.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    30u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    soul_gem(&mut plugin, EMPTY_SMALL, FILLED_SMALL, 2, 0);
    soul_gem(&mut plugin, EMPTY_COMMON, FILLED_COMMON, 3, 0);
    soul_gem(&mut plugin, EMPTY_GRAND, FILLED_GRAND, 5, 0);

    let mut order = LoadOrder::new();
    order.add("SoulTrap.esp", plugin).unwrap();
    let game = Game::new(order);
    for actor in [CASTER, TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game.world_mut().actor(id(TARGET)).unwrap().level = 18;
    game
}

fn cast(game: &mut Game) {
    game.shared_world()
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_44_soul_trap_fills_the_smallest_eligible_gem_once_on_death() {
    let mut game = fixture();
    {
        let mut world = game.world_mut();
        let inventory = &mut world.reference(id(CASTER)).unwrap().inventory;
        inventory.add(id(EMPTY_SMALL), 1).unwrap();
        inventory.add(id(EMPTY_COMMON), 1).unwrap();
        inventory.add(id(EMPTY_GRAND), 1).unwrap();
    }
    cast(&mut game);
    cast(&mut game);

    assert!(game.world_mut().kill_actor(id(TARGET), Some(id(CASTER))));
    game.pump();
    game.pump();

    let world = game.world();
    let inventory = &world.reference_ref(id(CASTER)).unwrap().inventory;
    assert_eq!(inventory.count(id(EMPTY_SMALL)), 1);
    assert_eq!(inventory.count(id(EMPTY_COMMON)), 0);
    assert_eq!(inventory.count(id(FILLED_COMMON)), 1);
    assert_eq!(inventory.count(id(EMPTY_GRAND)), 1);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_44_soul_trap_consumes_death_without_filling_an_ineligible_gem() {
    let mut game = fixture();
    game.world_mut()
        .reference(id(CASTER))
        .unwrap()
        .inventory
        .add(id(EMPTY_SMALL), 1)
        .unwrap();
    cast(&mut game);

    assert!(game.world_mut().kill_actor(id(TARGET), Some(id(CASTER))));
    game.pump();
    game.pump();

    let world = game.world();
    let inventory = &world.reference_ref(id(CASTER)).unwrap().inventory;
    assert_eq!(inventory.count(id(EMPTY_SMALL)), 1);
    assert_eq!(inventory.count(id(FILLED_SMALL)), 0);
    assert_eq!(game.world().active_effects().count(), 0);
}
