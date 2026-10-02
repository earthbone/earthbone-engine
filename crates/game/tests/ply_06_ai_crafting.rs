// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PLY-06 route ownership and shared workstation transactions.

use crate::common::installed;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{self, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::player_input::{Command, Confirmation, Driver, Error, STEP_SECONDS};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const TRAVEL: u32 = 0x810;
const SANDBOX: u32 = 0x811;
const USE: u32 = 0x812;
const BAD: u32 = 0x813;
const DEST: u32 = 0x820;
const FORGE: u32 = 0x821;
const LAB: u32 = 0x822;
const FORGE_KEY: u32 = 0x830;
const LAB_KEY: u32 = 0x831;
const INPUT: u32 = 0x840;
const OUTPUT: u32 = 0x841;
const RECIPE: u32 = 0x842;
const ING_A: u32 = 0x843;
const ING_B: u32 = 0x844;
const EFFECT: u32 = 0x845;
const CONDITIONAL: u32 = 0x814;
const SIT: u32 = 0x815;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn place(object: u32, base: u32, x: f32, tag: [u8; 4]) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), object)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn ingredient(object: u32) -> Record {
    Record::new(T(b"INGR"), object)
        .sub(T(b"ENIT"), [0; 8])
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(
            T(b"EFIT"),
            [
                12.0f32.to_le_bytes(),
                0u32.to_le_bytes(),
                10u32.to_le_bytes(),
            ]
            .concat(),
        )
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        BASE,
        "Player",
        "Player",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(place(PLAYER, BASE, 0.0, *b"ACHR"));
    for cell in [0x860, 0x861] {
        plugin.push_top(Record::new(T(b"CELL"), cell).sub(T(b"DATA"), [1, 0]));
    }
    for (form, procedure, target) in [
        (TRAVEL, "Travel", DEST),
        (SANDBOX, "Sandbox", DEST),
        (USE, "Activate", FORGE),
        (SIT, "Sit", FORGE),
        (BAD, "Unknown", DEST),
    ] {
        let template = package::finish(package::close_header(
            package::build_template(form + 0x100, procedure),
            None,
        ));
        plugin.push_top(package::with_procedure(template, procedure, &[0], true));
        let pack = package::close_header(
            package::build_record(form, procedure, 0, Schedule::default()),
            Some(form + 0x100),
        );
        let pack = if matches!(form, USE | SIT) {
            pack.zsub(T(b"ANAM"), "Target")
                .sub(
                    T(b"PTDA"),
                    [0i32.to_le_bytes(), target.to_le_bytes(), 0i32.to_le_bytes()].concat(),
                )
                .sub(T(b"UNAM"), [0])
        } else {
            package::with_location_input(pack, 0, Location::NearReference(target), 0)
        };
        plugin.push_top(package::finish(pack));
    }
    let condition = package::Condition {
        comparison: package::condition::EQUAL,
        flags: 0,
        value: package::CompValue::Float(1.0),
        function: 74,
        param1: 0x853,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let conditional = package::with_condition(
        package::build_record(CONDITIONAL, "Conditional", 0, Schedule::default()),
        &condition,
    );
    let conditional = package::close_header(conditional, Some(TRAVEL + 0x100));
    plugin.push_top(package::finish(package::with_location_input(
        conditional,
        0,
        Location::NearReference(DEST),
        0,
    )));
    plugin.push_top(
        Record::new(T(b"GLOB"), 0x853)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 0.0f32.to_le_bytes()),
    );
    plugin.push_top(place(DEST, 0, 30.0, *b"REFR"));
    for (reference, base, keyword, mode, name) in [
        (FORGE, FORGE + 0x100, FORGE_KEY, 1, "CraftingSmithingForge"),
        (LAB, LAB + 0x100, LAB_KEY, 5, "CraftingAlchemy"),
    ] {
        plugin.push_top(Record::new(T(b"KYWD"), keyword).zsub(T(b"EDID"), name));
        plugin.push_top(
            Record::new(T(b"FURN"), base)
                .sub(T(b"WBDT"), [mode, 0xff])
                .sub(T(b"MNAM"), 1u32.to_le_bytes())
                .sub(T(b"FNPR"), [1, 0, 0, 0])
                .sub(T(b"KWDA"), keyword.to_le_bytes()),
        );
        plugin.push_top(place(reference, base, 60.0, *b"REFR"));
    }
    for (form, name) in [(0x850, "AVSmithing"), (0x851, "AVAlchemy")] {
        plugin.push_top(
            Record::new(T(b"AVIF"), form).zsub(T(b"EDID"), name).sub(
                T(b"AVSK"),
                [2.0f32, 1.0, 10000.0, 100000.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
        );
    }
    plugin.push_top(
        Record::new(T(b"GMST"), 0x852)
            .zsub(T(b"EDID"), "fSkillUseCurve")
            .sub(T(b"DATA"), 1.0f32.to_le_bytes()),
    );
    for form in [INPUT, OUTPUT] {
        plugin.push_top(Record::new(T(b"MISC"), form).sub(
            T(b"DATA"),
            [20i32.to_le_bytes(), 0.0f32.to_le_bytes()].concat(),
        ));
    }
    plugin.push_top(
        Record::new(T(b"COBJ"), RECIPE)
            .sub(
                T(b"CNTO"),
                [INPUT.to_le_bytes(), 1i32.to_le_bytes()].concat(),
            )
            .sub(T(b"CNAM"), OUTPUT.to_le_bytes())
            .sub(T(b"BNAM"), FORGE_KEY.to_le_bytes())
            .sub(T(b"NAM1"), 1u16.to_le_bytes()),
    );
    plugin.push_top(ingredient(ING_A));
    plugin.push_top(ingredient(ING_B));
    let mut data = vec![0; 152];
    data[4..8].copy_from_slice(&5.0f32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), data));
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn run(game: &mut Game, commands: impl IntoIterator<Item = Command>) -> Driver {
    let mut driver = Driver::default();
    for command in commands {
        driver.push(command).unwrap();
    }
    for _ in 0..600 {
        let Some(event) = driver.step(game) else {
            return driver;
        };
        assert!(event.result.is_ok(), "{event:?}");
    }
    panic!("route exceeded ten simulation seconds");
}

#[test]
fn ply_06_route_waits_for_travel_completion_and_returns_player_control() {
    let mut game = game();
    let driver = run(
        &mut game,
        [
            Command::RunPackage {
                package: id(TRAVEL),
                duration: None,
            },
            Command::Turn {
                rotation: [0.0, 0.0, 1.0],
            },
        ],
    );
    assert!(driver.events().len() > 2, "travel must span multiple steps");
    assert_eq!(driver.events()[0].result, Ok("package running".into()));
    assert!(driver
        .events()
        .iter()
        .any(|event| event.result == Ok("package completed".into())));
    assert_eq!(
        game.world().reference_ref(id(PLAYER)).unwrap().position,
        [30.0, 0.0, 0.0]
    );
    assert!(!game.world().player_ai_driven);
    assert_eq!(game.world().count_package_overrides(id(PLAYER)), 0);
    assert_eq!(game.world().current_package(id(PLAYER)), None);
    assert!(game
        .world()
        .actor_ref(id(PLAYER))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(TRAVEL)));
    assert_eq!(driver.events().last().unwrap().rotation[2], 1.0);
}

#[test]
fn ply_06_player_ai_flag_gates_movement_and_manual_commands() {
    let mut game = game();
    let player = game.player();
    game.world_mut()
        .add_package_override(player, id(TRAVEL), 30, 0);
    game.tick(0.1);
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        [0.0; 3]
    );
    game.world_mut().set_player_ai_driven(true);
    game.tick(STEP_SECONDS);
    assert!(game.world().reference_ref(player).unwrap().position[0] > 0.0);
    assert!(matches!(
        game.apply_player_input(&Command::Turn { rotation: [0.0; 3] }),
        Err(Error::AiDriven)
    ));
    game.world_mut().set_player_ai_driven(false);
    let position = game.world().reference_ref(player).unwrap().position;
    game.tick(1.0);
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        position
    );
}

#[test]
fn ply_06_player_ai_motion_updates_cell_residency_and_pauses_in_menus() {
    let mut game = game();
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(id(0x860));
    game.world_mut().reference(id(DEST)).unwrap().cell = Some(id(0x861));
    game.change_cell(Some(id(0x860)));
    game.world_mut()
        .add_package_override(player, id(TRAVEL), 100, 1);
    game.world_mut().set_player_ai_driven(true);
    game.tick_menu_mode(10.0, true);
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        [0.0; 3]
    );
    assert_eq!(game.world().loaded_cell(), Some(id(0x860)));
    game.tick(1.0);
    assert_eq!(
        game.world().reference_ref(player).unwrap().cell,
        Some(id(0x861))
    );
    assert_eq!(game.world().loaded_cell(), Some(id(0x861)));
}

#[test]
fn ply_06_sandbox_duration_and_cancel_restore_prior_override_and_flag() {
    let mut game = game();
    let driver = run(
        &mut game,
        [Command::RunPackage {
            package: id(SANDBOX),
            duration: Some(0.1),
        }],
    );
    assert_eq!(
        driver.events().last().unwrap().result,
        Ok("package duration elapsed".into())
    );
    assert!(!game.world().player_ai_driven);
    assert!(!game
        .world()
        .actor_ref(id(PLAYER))
        .unwrap()
        .ai
        .completed
        .contains_key(&id(SANDBOX)));
    game.world_mut()
        .add_package_override(id(PLAYER), id(SANDBOX), 20, 0);
    let before = game
        .world()
        .actor_ref(id(PLAYER))
        .unwrap()
        .runtime_package_overrides
        .clone();
    game.world_mut().set_player_ai_driven(true);
    let mut driver = Driver::default();
    driver
        .push(Command::RunPackage {
            package: id(SANDBOX),
            duration: None,
        })
        .unwrap();
    driver.step(&mut game).unwrap();
    driver.cancel_package(&mut game);
    assert!(game.world().player_ai_driven);
    assert_eq!(
        game.world()
            .actor_ref(id(PLAYER))
            .unwrap()
            .runtime_package_overrides,
        before
    );
    assert_eq!(driver.pending(), 0);
}

#[test]
fn ply_06_bad_packages_durations_and_interruption_release_control() {
    let mut game = game();
    for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            Driver::default().push(Command::RunPackage {
                package: id(TRAVEL),
                duration: Some(duration)
            }),
            Err(Error::Duration)
        ));
    }
    for package in [id(0xffff), id(BAD)] {
        let mut driver = Driver::default();
        driver
            .push(Command::RunPackage {
                package,
                duration: None,
            })
            .unwrap();
        assert!(driver.step(&mut game).unwrap().result.is_err());
        assert!(!game.world().player_ai_driven);
        assert_eq!(game.world().count_package_overrides(id(PLAYER)), 0);
    }
    let mut driver = Driver::default();
    driver
        .push(Command::RunPackage {
            package: id(TRAVEL),
            duration: None,
        })
        .unwrap();
    driver.step(&mut game).unwrap();
    game.world_mut().set_player_ai_driven(false);
    assert!(driver.step(&mut game).unwrap().result.is_err());
    assert_eq!(driver.pending(), 0);
    assert_eq!(game.world().count_package_overrides(id(PLAYER)), 0);
}

#[test]
fn ply_06_overrides_use_priority_latest_tie_and_persist_in_actor_section() {
    let mut game = game();
    let player = game.player();
    for (package, priority) in [(TRAVEL, 30), (SANDBOX, 50), (USE, 50)] {
        assert!(game
            .world_mut()
            .add_package_override(player, id(package), priority, 0));
    }
    assert_eq!(
        ai::select(&mut game.world_mut(), player).unwrap().package,
        id(USE)
    );
    game.world_mut()
        .add_package_override(player, id(SANDBOX), 50, 0);
    assert_eq!(
        ai::select(&mut game.world_mut(), player).unwrap().package,
        id(SANDBOX)
    );
    let hash = game.world().state_hash().actors;
    let saved = game.save_state().unwrap();
    assert!(saved.windows(6).any(|bytes| bytes == b"PLY-06"));
    let mut restored = self::game();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world().state_hash().actors, hash);
    assert_eq!(
        ai::select(&mut restored.world_mut(), player)
            .unwrap()
            .package,
        id(SANDBOX)
    );
    assert_eq!(restored.world().count_package_overrides(player), 3);
    assert!(restored
        .world_mut()
        .remove_package_override(player, id(SANDBOX)));
    assert_eq!(
        restored.world_mut().remove_all_package_overrides(id(USE)),
        1
    );
    assert_eq!(restored.world_mut().clear_package_overrides(player), 1);
}

#[test]
fn ply_06_workstation_package_then_menu_craft_consumes_adds_and_grants_experience() {
    let mut game = game();
    game.world_mut().add_item(id(PLAYER), id(INPUT), 1);
    let driver = run(
        &mut game,
        [
            Command::RunPackage {
                package: id(USE),
                duration: None,
            },
            Command::ConfirmMenu(Confirmation::Craft {
                bench: id(FORGE),
                recipe: id(RECIPE),
            }),
        ],
    );
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(INPUT)), 0);
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(OUTPUT)), 1);
    assert!(
        game.world()
            .skill_experience(id(PLAYER), "smithing")
            .unwrap()
            > 0.0
    );
    assert!(driver
        .events()
        .iter()
        .any(|event| event.result == Ok("package completed".into())));
}

#[test]
fn ply_06_alchemy_menu_matches_shared_brew_and_grants_value_based_experience() {
    let mut bot = game();
    let mut menu = game();
    for game in [&mut bot, &mut menu] {
        for item in [ING_A, ING_B] {
            game.world_mut().add_item(id(PLAYER), id(item), 1);
        }
    }
    menu.brew_potion(id(PLAYER), id(LAB), &[id(ING_A), id(ING_B)])
        .unwrap();
    run(
        &mut bot,
        [Command::ConfirmMenu(Confirmation::Alchemy {
            bench: id(LAB),
            ingredients: [id(ING_A), id(ING_B)],
        })],
    );
    assert_eq!(
        bot.world().reference_ref(id(PLAYER)).unwrap().inventory,
        menu.world().reference_ref(id(PLAYER)).unwrap().inventory
    );
    let expected = (5.0 * 12.0f32.powf(1.1)).floor() * 2.0 + 1.0;
    assert_eq!(
        bot.world().skill_experience(id(PLAYER), "alchemy"),
        Some(expected)
    );
    assert_eq!(
        bot.world().skill_experience(id(PLAYER), "alchemy"),
        menu.world().skill_experience(id(PLAYER), "alchemy")
    );
    let saved = bot.save_state().unwrap();
    bot.load_state(&saved).unwrap();
    assert_eq!(
        bot.world().skill_experience(id(PLAYER), "alchemy"),
        Some(expected)
    );
}

#[test]
fn ply_06_rejected_workstation_craft_preserves_inventory_and_experience() {
    let mut game = game();
    game.world_mut().drive_actor(id(PLAYER));
    game.world_mut().add_item(id(PLAYER), id(ING_A), 1);
    let before = game
        .world()
        .reference_ref(id(PLAYER))
        .unwrap()
        .inventory
        .clone();
    let experience = game.world().skill_experience(id(PLAYER), "alchemy");
    let events = game.world().pending.len();
    assert!(game
        .apply_player_input(&Command::ConfirmMenu(Confirmation::Alchemy {
            bench: id(LAB),
            ingredients: [id(ING_A), id(ING_B)]
        }))
        .is_err());
    assert_eq!(
        game.world().reference_ref(id(PLAYER)).unwrap().inventory,
        before
    );
    assert_eq!(
        game.world().skill_experience(id(PLAYER), "alchemy"),
        experience
    );
    assert_eq!(game.world().pending.len(), events);
    assert!(game
        .apply_player_input(&Command::ConfirmMenu(Confirmation::Craft {
            bench: id(LAB),
            recipe: id(RECIPE)
        }))
        .is_err());
    assert_eq!(
        game.world().reference_ref(id(PLAYER)).unwrap().inventory,
        before
    );
    assert_eq!(
        game.world().skill_experience(id(PLAYER), "smithing"),
        Some(0.0)
    );
    assert_eq!(game.world().pending.len(), events);
}

#[test]
fn ply_06_brewing_progression_error_preserves_components_and_notifications() {
    let mut game = game();
    let player = game.player();
    for item in [ING_A, ING_B] {
        game.world_mut().add_item(player, id(item), 1);
    }
    game.world_mut()
        .actor(player)
        .unwrap()
        .skill_experience
        .insert("alchemy".into(), f32::INFINITY);
    let inventory = game
        .world()
        .reference_ref(player)
        .unwrap()
        .inventory
        .clone();
    let events = game.world().pending.len();
    assert!(matches!(
        game.brew_potion(player, id(LAB), &[id(ING_A), id(ING_B)]),
        Err(soe_game::crafting::Error::Progression(_))
    ));
    assert_eq!(
        game.world().reference_ref(player).unwrap().inventory,
        inventory
    );
    assert_eq!(game.world().pending.len(), events);
}

fn call(game: &mut Game, class: &str, name: &str, args: &[Value]) -> Value {
    let script = format!("Ply06{class}{name}{}", game.vm.log.entries.len());
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s(class), s(name), ident("result")];
    for (index, argument) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        let kind = match argument {
            Value::Int(_) => "Int",
            Value::Bool(_) => "Bool",
            _ => "Form",
        };
        function = function.param(&parameter, kind);
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(
            function
                .ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn ply_06_papyrus_ai_handoff_and_actorutil_natives_use_shared_overrides() {
    let mut game = game();
    let player = Value::Object(game.object_of_form(id(PLAYER)));
    let package = Value::Object(game.object_of_form(id(TRAVEL)));
    assert_eq!(
        call(
            &mut game,
            "ActorUtil",
            "AddPackageOverride",
            &[player.clone(), package.clone()]
        ),
        Value::None
    );
    assert_eq!(
        game.world()
            .actor_ref(id(PLAYER))
            .unwrap()
            .runtime_package_overrides[0]
            .priority,
        30
    );
    assert_eq!(
        call(
            &mut game,
            "ActorUtil",
            "CountPackageOverride",
            std::slice::from_ref(&player)
        ),
        Value::Int(1)
    );
    call(&mut game, "Game", "SetPlayerAIDriven", &[Value::Bool(true)]);
    game.tick(STEP_SECONDS);
    assert_eq!(
        game.world().actor_ref(id(PLAYER)).unwrap().ai.procedure,
        Procedure::Travel
    );
    assert_eq!(
        call(
            &mut game,
            "ActorUtil",
            "RemovePackageOverride",
            &[player.clone(), package.clone()]
        ),
        Value::Bool(true)
    );
    call(
        &mut game,
        "ActorUtil",
        "AddPackageOverride",
        &[player.clone(), package.clone()],
    );
    assert_eq!(
        call(
            &mut game,
            "ActorUtil",
            "RemoveAllPackageOverride",
            std::slice::from_ref(&package)
        ),
        Value::Int(1)
    );
    call(
        &mut game,
        "ActorUtil",
        "AddPackageOverride",
        &[player.clone(), package],
    );
    assert_eq!(
        call(&mut game, "ActorUtil", "ClearPackageOverride", &[player]),
        Value::Int(1)
    );
    call(
        &mut game,
        "Game",
        "SetPlayerAIDriven",
        &[Value::Bool(false)],
    );
    assert!(!game.world().player_ai_driven);
}

#[test]
fn ply_06_override_eligibility_force_priority_clamp_and_completion_cleanup() {
    let mut game = game();
    let player = game.player();
    game.world_mut()
        .add_package_override(player, id(TRAVEL), -20, 0);
    game.world_mut()
        .add_package_override(player, id(CONDITIONAL), 300, 2);
    let entries = game
        .world()
        .actor_ref(player)
        .unwrap()
        .runtime_package_overrides
        .clone();
    assert_eq!((entries[0].priority, entries[1].priority), (0, 100));
    assert!(!entries[1].force);
    assert_eq!(ai::select(&mut game.world_mut(), player), None);
    assert_eq!(game.world().count_package_overrides(player), 2);
    game.world_mut()
        .add_package_override(player, id(CONDITIONAL), 100, 1);
    assert_eq!(
        ai::select(&mut game.world_mut(), player).unwrap().package,
        id(CONDITIONAL)
    );
    game.world_mut().set_player_ai_driven(true);
    game.tick(1.0);
    assert!(
        game.world()
            .actor_ref(player)
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
    assert_eq!(game.world().count_package_overrides(player), 1);
    assert_eq!(
        ai::select(&mut game.world_mut(), player).unwrap().package,
        id(TRAVEL)
    );
}

#[test]
fn ply_06_installed_vanilla_travel_and_sandbox_packages_return_control() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let player = game.player();
    let travel = game.form("WhiterunAdrianneEstablishingScene").unwrap();
    let target = match ai::procedure_of(&game.world(), travel).1.unwrap().location {
        Location::NearReference(raw) => game.world().resolve_from(travel, raw).unwrap(),
        other => panic!("unexpected vanilla Travel location: {other:?}"),
    };
    let driver = run(
        &mut game,
        [
            Command::MoveTo {
                target,
                offset: [30.0, 0.0, 0.0],
                match_rotation: false,
            },
            Command::RunPackage {
                package: travel,
                duration: None,
            },
        ],
    );
    assert!(driver
        .events()
        .iter()
        .any(|event| event.result == Ok("package completed".into())));
    assert!(!game.world().player_ai_driven);
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        game.world().reference_ref(target).unwrap().position
    );
    let sandbox = game
        .form("WhiterunAdrianneSandboxForgePackage6x11")
        .unwrap();
    let target = match ai::procedure_of(&game.world(), sandbox).1.unwrap().location {
        Location::NearReference(raw) => game.world().resolve_from(sandbox, raw).unwrap(),
        other => panic!("unexpected vanilla Sandbox location: {other:?}"),
    };
    run(
        &mut game,
        [Command::MoveTo {
            target,
            offset: [12.0, 0.0, 0.0],
            match_rotation: false,
        }],
    );
    let mut driver = Driver::default();
    driver
        .push(Command::RunPackage {
            package: sandbox,
            duration: Some(1.0),
        })
        .unwrap();
    let mut saw_sandbox = false;
    for _ in 0..120 {
        let Some(event) = driver.step(&mut game) else {
            break;
        };
        assert!(event.result.is_ok(), "{event:?}");
        saw_sandbox |= game.world().actor_ref(player).unwrap().ai.procedure == Procedure::Sandbox;
    }
    assert!(
        saw_sandbox,
        "vanilla tree must progress from Travel to Sandbox"
    );
    assert_eq!(driver.pending(), 0);
    assert_eq!(
        driver.events().last().unwrap().result,
        Ok("package duration elapsed".into())
    );
    assert!(!game.world().player_ai_driven);
    assert_eq!(game.world().count_package_overrides(player), 0);
    assert!(!game
        .world()
        .actor_ref(player)
        .unwrap()
        .ai
        .completed
        .contains_key(&sandbox));
}

#[test]
fn ply_06_sit_target_workstation_reserves_walks_and_completes() {
    let mut game = game();
    let mut driver = Driver::default();
    driver
        .push(Command::RunPackage {
            package: id(SIT),
            duration: None,
        })
        .unwrap();
    assert!(driver.step(&mut game).unwrap().result.is_ok());
    assert_eq!(
        game.world().furniture_seat(id(PLAYER)),
        Some((id(FORGE), 0))
    );
    while let Some(event) = driver.step(&mut game) {
        assert!(event.result.is_ok(), "{event:?}");
        assert!(driver.events().len() < 120);
    }
    assert_eq!(game.world().furniture_seat(id(PLAYER)), None);
    assert!(!game.world().player_ai_driven);
    assert_eq!(
        game.world().reference_ref(id(PLAYER)).unwrap().position,
        [60.0, 0.0, 0.0]
    );
}

#[test]
fn ply_06_installed_warmaidens_forge_and_breezehome_lab_craft_vanilla_inputs() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let player = game.player();
    let forge = id(0x1ad6e);
    let dagger = id(0x1397e);
    let forge_base = game.world_mut().reference(forge).unwrap().base.unwrap();
    let keyword = game.form("CraftingSmithingForge").unwrap();
    assert!(game.world_mut().has_keyword(forge_base, keyword));
    let (recipe, definition) = game
        .world()
        .order()
        .winners_of(T(b"COBJ"))
        .find_map(|(form, record)| {
            let recipe = soe_esm::records::gameplay::Recipe::from_record(record).ok()?;
            (recipe
                .created_object
                .and_then(|raw| game.world().resolve_from(form, raw))
                == Some(dagger)
                && recipe
                    .workbench_keyword
                    .and_then(|raw| game.world().resolve_from(form, raw))
                    == Some(keyword))
            .then_some((form, recipe))
        })
        .expect("vanilla iron dagger forge recipe");
    game.world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .clear();
    let mut components = Vec::new();
    for input in definition.ingredients {
        let form = game.world().resolve_from(recipe, input.item).unwrap();
        game.world_mut().add_item(player, form, input.count);
        components.push(form);
    }
    let workstation_package = game.form("C06EorlundReforgeWuuthrad").unwrap();
    run(
        &mut game,
        [
            Command::MoveTo {
                target: forge,
                offset: [30.0, 0.0, 0.0],
                match_rotation: false,
            },
            Command::RunPackage {
                package: workstation_package,
                duration: None,
            },
            Command::ConfirmMenu(Confirmation::Craft {
                bench: forge,
                recipe,
            }),
        ],
    );
    for input in components {
        assert_eq!(game.world_mut().item_count(player, input), 0);
    }
    assert_eq!(game.world_mut().item_count(player, dagger), 1);
    assert!(game.world().skill_experience(player, "smithing").unwrap() > 0.0);
    let lab = id(0xe4c31);
    let cell = game.form("WhiterunBreezehome").unwrap();
    {
        let mut world = game.world_mut();
        let reference = world.reference(lab).unwrap();
        assert_eq!(reference.cell, Some(cell));
        reference.enabled = true;
        reference.deleted = false;
    }
    for ingredient in [id(0x77e1c), id(0x4b0ba)] {
        game.world_mut().add_item(player, ingredient, 1);
    }
    run(
        &mut game,
        [
            Command::MoveTo {
                target: lab,
                offset: [0.0; 3],
                match_rotation: false,
            },
            Command::ConfirmMenu(Confirmation::Alchemy {
                bench: lab,
                ingredients: [id(0x77e1c), id(0x4b0ba)],
            }),
        ],
    );
    assert_eq!(game.world_mut().item_count(player, id(0x77e1c)), 0);
    assert_eq!(game.world_mut().item_count(player, id(0x4b0ba)), 0);
    let potion = game
        .world()
        .reference_ref(player)
        .unwrap()
        .inventory
        .iter()
        .find_map(|(form, count)| {
            (game.world().tag_of(*form) == Some(T(b"ALCH")) && count == 1).then_some(*form)
        })
        .expect("one brewed vanilla ingredient potion");
    assert_eq!(game.world().ingestible(potion).unwrap().effects.len(), 2);
    let experience = game.world().skill_experience(player, "alchemy").unwrap();
    assert!(experience > 0.0);
    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    assert_eq!(game.world_mut().item_count(player, potion), 1);
    assert_eq!(
        game.world().skill_experience(player, "alchemy"),
        Some(experience)
    );
}
