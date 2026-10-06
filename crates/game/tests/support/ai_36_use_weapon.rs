// SPDX-License-Identifier: GPL-3.0-or-later
// AI-36 covers package-authored UseWeapon calls through the common combat path.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACKAGE: u32 = 0x801;
const BASE: u32 = 0x802;
const ATTACKER: u32 = 0x803;
const TARGET: u32 = 0x804;
const WEAPON: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn weapon_record() -> Record {
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8_f32.to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1.1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    Record::new(T(b"WEAP"), WEAPON)
        .sub(T(b"DATA"), data)
        .sub(T(b"DNAM"), dnam)
}

fn game() -> Game {
    let tree = package::close_header(package::build_template(TEMPLATE, "UseWeapon"), None);
    let tree = package::finish(tree);
    let tree = package::with_procedure(tree, "UseWeapon", &[], true);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(package::finish(tree));
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACKAGE, "AttackTarget", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Combatant",
            "Combatant",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"ACHR"), ATTACKER).sub(T(b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"ACHR"), TARGET).sub(T(b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(weapon_record());
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(T(b"GMST"), 0x900 + index as u32)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("AI36.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        assert!(world.drive_actor(id(ATTACKER)));
        assert!(world.drive_actor(id(TARGET)));
        for actor in [id(ATTACKER), id(TARGET)] {
            world.set_actor_value(actor, "health", 100.).unwrap();
            world.set_actor_value(actor, "onehanded", 100.).unwrap();
        }
        world.add_item(id(ATTACKER), id(WEAPON), 1);
        let attacker = world.actor(id(ATTACKER)).unwrap();
        attacker.right_weapon = Some(id(WEAPON));
        attacker.equipped.insert(id(WEAPON));
        world.actor(id(ATTACKER)).unwrap().combat_target = Some(id(TARGET));
    }
    game
}

#[test]
fn ai_36_authored_useweapon_reaches_shared_melee_path() {
    let mut game = game();
    let before = game.world_mut().actor_value(id(TARGET), "health");
    game.tick(0.1);
    let after = game.world_mut().actor_value(id(TARGET), "health");
    assert!(after < before, "UseWeapon did not damage its combat target");
    assert_eq!(
        game.world().actor_ref(id(ATTACKER)).unwrap().ai.procedure,
        Procedure::UseWeapon
    );
    assert!(
        game.world()
            .actor_ref(id(ATTACKER))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
}

#[test]
fn ai_36_useweapon_state_round_trips_at_save_version_37() {
    let mut game = game();
    game.tick(0.1);
    let before = game
        .world()
        .actor_ref(id(ATTACKER))
        .unwrap()
        .ai
        .procedure
        .clone();
    assert_eq!(before, Procedure::UseWeapon);
    let bytes = game.world_mut().save_state();
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 37);
    game.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        game.world().actor_ref(id(ATTACKER)).unwrap().ai.procedure,
        Procedure::UseWeapon
    );
}

#[test]
fn prf_27_weapon_choice_timing_wraps_an_actual_useweapon_contact() {
    let mut game = game();
    let before = game.world_mut().actor_value(id(TARGET), "health");
    game.tick(0.1);
    assert!(game.world_mut().actor_value(id(TARGET), "health") < before);
    assert!(game.world().ai.planning_phases["weapon_choice"] > std::time::Duration::ZERO);
    assert!(game.world().ai.planning_phases["procedure.UseWeapon"] > std::time::Duration::ZERO);
}
