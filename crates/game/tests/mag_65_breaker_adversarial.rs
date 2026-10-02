#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CASTER: u32 = 0x900;
const TARGET: u32 = 0x901;
const VALID: u32 = 0xA00;
const DISGUISE: u32 = 0xA01;
const SPELL: u32 = 0xA02;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn effect(archetype: u32) -> Record {
    let mut data = vec![0; 152];
    data[64..68].copy_from_slice(&archetype.to_le_bytes());
    data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    Record::new(T(b"MGEF"), if archetype == 44 { DISGUISE } else { VALID })
        .sub(T(b"DATA"), data)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [CASTER, TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    plugin.push_top(effect(0));
    plugin.push_top(effect(44));
    let mut spit = vec![0; 36];
    spit[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spit[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let efit = [20.0_f32.to_le_bytes(), 0_u32.to_le_bytes(), 3_u32.to_le_bytes()].concat();
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), VALID.to_le_bytes())
            .sub(T(b"EFIT"), efit.clone())
            .sub(T(b"EFID"), DISGUISE.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Disguise.esm", plugin).expect("valid fixture");
    Game::new(order)
}

#[test]
fn mag_65_rejecting_disguise_keeps_prior_spell_effect_unapplied() {
    let mut game = fixture();
    let world = game.shared_world();
    let result = world.borrow_mut().cast_spell(
        id(SPELL),
        id(CASTER),
        Some(id(CASTER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        matches!(
            result,
            Err(soe_game::world::magic::MagicError::Unsupported("effect archetype"))
        ),
        "unexpected cast result: {result:?}"
    );
    assert_eq!(game.world().active_effects().count(), 0);
}
