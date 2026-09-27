// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(dead_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::magic::MagicError, Game};

pub const ACTOR: u32 = 0x100;
pub const OTHER: u32 = 0x101;
pub const SOURCE: u32 = 0x102;
pub const EFFECT: u32 = 0x200;
pub const SPELL: u32 = 0x300;
pub const T: fn(&[u8; 4]) -> Tag = Tag::new;

pub fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

pub fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn effect(flags: u32, resistance: i32) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 16, resistance as u32);
    put(&mut data, 64, 0);
    put(&mut data, 68, 24);
    put(&mut data, 80, 0);
    put(&mut data, 84, 0);
    Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), data)
}

#[allow(dead_code)]
pub fn spell(form: u32, cast_type: u32, delivery: u32, range: f32, area: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, 0);
    put(&mut data, 16, cast_type);
    put(&mut data, 20, delivery);
    data[28..32].copy_from_slice(&range.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(
            T(b"EFIT"),
            [
                100.0f32.to_le_bytes().as_slice(),
                &area.to_le_bytes(),
                &0u32.to_le_bytes(),
            ]
            .concat(),
        )
}

pub fn game(mut effect: Record, spells: Vec<Record>, positions: &[(u32, [f32; 3])]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [ACTOR, OTHER, SOURCE] {
        let position = positions
            .iter()
            .find(|(form, _)| *form == actor)
            .map_or([0.0; 3], |(_, p)| *p);
        let mut data = vec![0; 24];
        for (i, value) in position.into_iter().enumerate() {
            data[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    if let (Some(record), Some(spell)) = (
        effect
            .subrecords
            .iter_mut()
            .find(|sub| sub.tag == T(b"DATA")),
        spells.first(),
    ) {
        if let Some(spit) = spell.find(T(b"SPIT")) {
            record.data[80..84].copy_from_slice(&spit[16..20]);
            record.data[84..88].copy_from_slice(&spit[20..24]);
        }
    }
    plugin.push_top(effect);
    for spell in spells {
        plugin.push_top(spell);
    }
    let mut order = LoadOrder::new();
    order.add("MagicAdversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

#[allow(dead_code)]
pub fn cast(
    game: &mut Game,
    spell: u32,
    source: u32,
    target: u32,
) -> Result<Vec<soe_game::world::magic::EffectId>, MagicError> {
    let shared = game.shared_world();
    let vm = &mut game.vm;
    let result = shared.borrow_mut().cast_spell(
        id(spell),
        id(source),
        Some(id(source)),
        Some(id(target)),
        &mut |name, class| vm.create_object_of(name, class),
    );
    result
}
