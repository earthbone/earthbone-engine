// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::detection::{Motion, Observation};
use soe_papyrus::asm::ScriptBuilder;

const HUNTER: u32 = 0x100;
const FRIEND: u32 = 0x101;
const HUNTER_BASE: u32 = 0x200;
const FRIEND_BASE: u32 = 0x201;
const EFFECT: u32 = 0x300;
const SPELL: u32 = 0x301;
const RECOVER: u32 = 1 << 1;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn gmst_f(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn gmst_i(object: u32, name: &str, value: i32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn actor_base(object: u32, aggression: u8) -> Record {
    let mut ai_data = vec![aggression, 4, 70, 1, 3, 2, 0, 0];
    for radius in [500_u32, 300, 100] {
        ai_data.extend_from_slice(&radius.to_le_bytes());
    }
    Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"AIDT"), ai_data)
}

fn actor_ref(object: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0_u8; 12]);
    Record::new(Tag(*b"ACHR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

fn fixture() -> Game {
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 0, RECOVER);
    put(&mut effect_data, 64, 8);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 0);

    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    put(&mut spell_data, 20, 0);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(actor_base(HUNTER_BASE, 1));
    plugin.push_top(actor_base(FRIEND_BASE, 0));
    plugin.push_top(actor_ref(HUNTER, HUNTER_BASE, [5.0, 10.0, 0.0]));
    plugin.push_top(actor_ref(FRIEND, FRIEND_BASE, [80.0, 80.0, 0.0]));
    for (index, (name, value)) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(gmst_f(0x400 + index as u32, name, value));
    }
    plugin.push_top(gmst_i(0x500, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0x501,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT).sub(Tag(*b"DATA"), effect_data));
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(
                Tag(*b"EFIT"),
                [
                    3.0_f32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    2_u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Frenzy.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [HUNTER, FRIEND] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game.world_mut().actor(id(FRIEND)).unwrap().teammate = true;
    game.load_script(&ScriptBuilder::new("FrenzyProbe", "ActiveMagicEffect").bytes())
        .unwrap();
    game.bind();
    game
}

fn detect_friend(game: &mut Game) {
    game.world_mut()
        .evaluate_detection(
            &[Observation {
                observer: id(HUNTER),
                target: id(FRIEND),
                distance: 1.0,
                line_of_sight: true,
                light_level: 100.0,
                motion: Motion::Still,
                action_sound: 0.0,
                action_sound_distance: None,
                action_sound_line_of_sight: None,
            }],
            1.0,
        )
        .unwrap();
}

fn cast_frenzy(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(HUNTER),
            Some(id(HUNTER)),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_31_frenzy_allows_combat_targeting_of_a_teammate() {
    let mut game = fixture();
    detect_friend(&mut game);
    cast_frenzy(&mut game);
    assert_eq!(game.world_mut().actor_value(id(HUNTER), "aggression"), 4.0);

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    assert_eq!(
        game.world().actor_ref(id(HUNTER)).unwrap().combat_target,
        Some(id(FRIEND))
    );
}

#[test]
fn mag_31_frenzy_expiry_releases_its_combat_target_override() {
    let mut game = fixture();
    detect_friend(&mut game);
    cast_frenzy(&mut game);
    assert_eq!(game.world_mut().actor_value(id(HUNTER), "aggression"), 4.0);
    {
        let shared = game.shared_world();
        shared
            .borrow_mut()
            .ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));
    }
    assert_eq!(
        game.world().actor_ref(id(HUNTER)).unwrap().combat_target,
        Some(id(FRIEND))
    );

    let shared = game.shared_world();
    shared.borrow_mut().tick_magic(2.0, &mut |name, class| {
        game.vm.create_object_of(name, class)
    });
    shared
        .borrow_mut()
        .ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    let world = game.world();
    let hunter = world.actor_ref(id(HUNTER)).unwrap();
    assert_eq!(hunter.combat_target, None);
    assert!(!hunter.in_combat);
    drop(world);
    assert_eq!(game.world_mut().actor_value(id(HUNTER), "aggression"), 1.0);
}
