// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::MagicError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TARGET: u32 = 0x901;
const EFFECT: u32 = 0xA00;
const SPELL: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    plugin.push_top(Record::new(T(b"ACHR"), TARGET).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    20.0_f32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    3_u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order
        .add("Disguise.esm", plugin)
        .expect("fixture load order");
    Game::new(order)
}

#[test]
fn mag_65_missing_disguise_effect_record_is_rejected_without_mutation() {
    let disguised = game();
    let cast = disguised.shared_world().borrow_mut().cast_spell(
        id(SPELL),
        id(TARGET),
        Some(id(TARGET)),
        Some(id(TARGET)),
        &mut |_, _| soe_papyrus::ObjectId(1),
    );
    assert!(matches!(cast, Err(MagicError::Missing(form)) if form == id(SPELL)));
    assert_eq!(disguised.world().active_effects().count(), 0);
}
