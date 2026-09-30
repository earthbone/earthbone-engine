#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const OBSERVER: u32 = 0x900;
const TARGET: u32 = 0x901;
const OTHER_TARGET: u32 = 0x902;
const EFFECT: u32 = 0xA00;
const SPELL: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x800));
    for actor in [OBSERVER, TARGET, OTHER_TARGET] {
        plugin.push_top(Record::new(T(b"ACHR"), actor).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    }
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 64, 44);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 1);
    put(&mut spit, 20, 3);
    let mut efit = 20.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let floats: [(&str, f32); 24] = [
        ("fSneakBaseValue", 0.0),
        ("fSneakMaxDistance", 100.0),
        ("fSneakDistanceAttenuationExponent", 1.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 0.0),
        ("fSneakEquippedWeightMult", 1.0),
        ("fSneakRunningMult", 1.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.0),
        ("fSneakLightRunMult", 0.0),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 1.0),
        ("fSneakStealthBoyMult", 1.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 100.0),
        ("fSneakAlertMod", 0.0),
        ("fSneakCombatMod", 0.0),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 5.0),
        ("fCombatStealthPointRegenLostWaitTime", 5.0),
    ];
    for (index, (name, value)) in floats.into_iter().enumerate() {
        plugin.push_top(
            Record::new(T(b"GMST"), 0xB00 + index as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    for (form, name, value) in [
        (0xB20, "iCombatStealthPointDetectionThreshold", 50_i32),
        (0xB21, "iCombatStealthPointSneakDetectionThreshold", 70_i32),
    ] {
        plugin.push_top(
            Record::new(T(b"GMST"), form)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add("Disguise.esm", plugin)
        .expect("fixture load order is valid");
    let game = Game::new(order);
    for actor in [OBSERVER, TARGET, OTHER_TARGET] {
        game.world_mut()
            .set_actor_value(id(actor), "sneak", 0.0)
            .expect("fixture actor has a Sneak value");
    }
    game
}

#[test]
fn mag_65_disguise_with_no_archetype_data_expires() {
    let mut game = fixture();
    let shared = game.shared_world();
    let applied = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(OBSERVER),
        Some(id(OBSERVER)),
        Some(id(TARGET)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    applied.expect("fixture cast succeeds");
    assert_eq!(game.world().active_effects().count(), 1);
    game.tick(3.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
