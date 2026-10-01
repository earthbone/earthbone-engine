// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared archetype fixtures from the game integration-test record builders.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

pub const CASTER: u32 = 0x100;
pub const TARGET: u32 = 0x101;
pub const OTHER: u32 = 0x102;
pub const BASE: u32 = 0x200;
pub const UNDEAD_KEYWORD: u32 = 0x202;

pub fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

pub fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn effect(form: u32, archetype: u32, flags: u32, primary_av: u32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, primary_av);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(Tag(*b"MGEF"), form).sub(Tag(*b"DATA"), data)
}

pub fn spell(form: u32, effect: u32, duration: u32, magnitude: f32, cast_type: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, cast_type);
    put(&mut data, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(Tag(*b"SPEL"), form)
        .sub(Tag(*b"SPIT"), data)
        .sub(Tag(*b"EFID"), effect.to_le_bytes())
        .sub(Tag(*b"EFIT"), efit)
}

pub fn actor(form: u32, position: [f32; 3]) -> Record {
    actor_with_base(form, BASE, position)
}

fn actor_with_base(form: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = vec![0; 24];
    for (axis, value) in position.into_iter().enumerate() {
        data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(Tag(*b"ACHR"), form)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

pub fn fixture(effects: Vec<Record>, spells: Vec<Record>, undead: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), vec![0; 24]));
    if undead {
        let npc = Record::new(Tag(*b"NPC_"), BASE + 1)
            .sub(Tag(*b"ACBS"), vec![0; 24])
            .sub(Tag(*b"KWDA"), UNDEAD_KEYWORD.to_le_bytes());
        plugin.push_top(npc);
        plugin.push_top(
            Record::new(Tag(*b"KYWD"), UNDEAD_KEYWORD).zsub(Tag(*b"EDID"), "ActorTypeUndead"),
        );
    }
    for form in [CASTER, TARGET, OTHER] {
        let base = if undead && form == TARGET {
            BASE + 1
        } else {
            BASE
        };
        let position = if form == CASTER {
            [0.0, 0.0, 0.0]
        } else {
            [8.0, 0.0, 0.0]
        };
        plugin.push_top(actor_with_base(form, base, position));
    }
    for effect in effects {
        plugin.push_top(effect);
    }
    for spell in spells {
        plugin.push_top(spell);
    }
    let mut order = LoadOrder::new();
    order.add("MagicAdversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    for form in [CASTER, TARGET, OTHER] {
        for (name, value) in [
            ("health", 100.0),
            ("magicka", 100.0),
            ("aggression", 1.0),
            ("confidence", 1.0),
        ] {
            game.world_mut()
                .set_actor_value(id(form), name, value)
                .unwrap();
        }
    }
    game
}

pub fn cast(game: &mut Game, spell: u32, target: u32) -> Vec<soe_game::world::magic::EffectId> {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .cast_spell(
            id(spell),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(target)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    result
}
