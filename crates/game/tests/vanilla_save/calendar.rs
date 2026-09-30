// SPDX-License-Identifier: GPL-3.0-or-later

//! Calendar imports use the six globals named by CommonLib Calendar.

use super::*;
use soe_game::clock::{Calendar, CALENDAR_GLOBALS};

fn calendar_order() -> LoadOrder {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into(), "A.esp".into()],
        ..Default::default()
    });
    for (i, name) in CALENDAR_GLOBALS.iter().enumerate() {
        patch.push_top(
            Record::new(Tag(*b"GLOB"), 0x0100_0900 + i as u32)
                .sub(Tag(*b"EDID"), [name.as_bytes(), &[0]].concat())
                .sub(Tag(*b"FLTV"), 0.0_f32.to_le_bytes().to_vec()),
        );
    }
    order.add("Calendar.esp", patch).unwrap();
    order
}

fn calendar_save(values: [f32; 6]) -> Save {
    let mut save = save();
    let values: Vec<_> = values
        .into_iter()
        .enumerate()
        .map(|(i, v)| (0x0100_0900 + i as u32, v))
        .collect();
    globals(&mut save, &values);
    save
}

#[test]
fn sav_08_restores_calendar_before_execution_and_preserves_it_in_engine_saves() {
    let mut game = Game::new(calendar_order());
    let save = calendar_save([205.0, 11.0, 31.0, 23.5, 42.125, 60.0]);
    game.world_mut().clock.real_seconds = 123.0;
    game.load_script(
        &ScriptBuilder::new("CalendarProbe", "")
            .function(
                FunctionBuilder::new("Read", "Float")
                    .global()
                    .local("result", "Float")
                    .ins(
                        Opcode::CallStatic,
                        vec![s("Utility"), s("GetCurrentGameTime"), ident("result")],
                    )
                    .ins(Opcode::Return, vec![ident("result")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global("CalendarProbe", "Read", &[]).unwrap();
    let timer_object = game.object_of_form(id(3, 0x900));
    game.world_mut()
        .register_update(timer_object, 1.0, false, true);
    let vm_before = game.vm.save_state().unwrap();
    let pending = format!("{:?}", game.world().pending);
    assert_eq!(game.world_mut().restore_vanilla_globals(&save).unwrap(), 6);
    assert_eq!(game.vm.save_state().unwrap(), vm_before);
    assert_eq!(format!("{:?}", game.world().pending), pending);
    assert_eq!(game.world().clock.real_seconds, 123.0);
    assert_eq!(game.world().clock.game_days, 42.125);
    assert_eq!(game.world().clock.timescale, 60.0);
    assert_eq!(
        game.world().calendar(),
        Calendar {
            year: 205,
            month: 11,
            day: 31,
            hour: 23.5
        }
    );
    let condition = soe_esm::records::gameplay::Condition {
        flags: 0,
        comparison: soe_esm::records::gameplay::Comparison::Number(23.5),
        function: 18,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    let context = soe_game::conditions::Context {
        owner: id(3, 0x900),
        subject: id(3, 0x900),
        target: None,
        quest: None,
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    };
    assert!(soe_game::conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        context,
        &condition
    )
    .unwrap());
    let bytes = game.save_state().unwrap();
    let hash = game.world().state_hash();
    let mut restored = Game::new(calendar_order());
    restored
        .load_script(
            &ScriptBuilder::new("CalendarProbe", "")
                .function(
                    FunctionBuilder::new("Read", "Float")
                        .global()
                        .local("result", "Float")
                        .ins(
                            Opcode::CallStatic,
                            vec![s("Utility"), s("GetCurrentGameTime"), ident("result")],
                        )
                        .ins(Opcode::Return, vec![ident("result")])
                        .build(),
                )
                .bytes(),
        )
        .unwrap();
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world().calendar(), game.world().calendar());
    assert_eq!(restored.world().state_hash(), hash);
    assert_eq!(restored.save_state().unwrap(), bytes);
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(42.125)));
    assert!(game.world().pending.is_empty());
    game.world_mut().fire_timers();
    assert!(
        matches!(game.world().pending.as_slice(), [soe_game::world::Pending::Event { object, name, .. }]
        if *object == timer_object && name == "OnUpdateGameTime")
    );
}

#[test]
fn calendar_rejection_is_atomic_for_missing_nonfinite_and_out_of_range_values() {
    let game = Game::new(calendar_order());
    let before = game.save_state().unwrap();
    let valid = [201.0, 1.0, 28.0, 23.0, 123.5, 20.0];
    for (index, bad) in [
        (0, -1.0),
        (0, 201.5),
        (1, 12.0),
        (1, -1.0),
        (1, 1.5),
        (2, 29.0),
        (2, 0.0),
        (2, 2.5),
        (3, 24.0),
        (3, -1.0),
        (4, -1.0),
        (5, -1.0),
    ] {
        let mut values = valid;
        values[index] = bad;
        assert!(matches!(
            game.world_mut()
                .restore_vanilla_globals(&calendar_save(values)),
            Err(RestoreError::Calendar(_))
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
    for index in 0..6 {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut values = valid;
            values[index] = bad;
            assert!(matches!(
                game.world_mut()
                    .restore_vanilla_globals(&calendar_save(values)),
                Err(RestoreError::Calendar(_))
            ));
            assert_eq!(game.save_state().unwrap(), before);
        }
        let mut save = calendar_save(valid);
        let mut values = save.globals().unwrap();
        values.variables.remove(index);
        save.table_1[0].data = values.write().unwrap();
        assert!(matches!(
            game.world_mut().restore_vanilla_globals(&save),
            Err(RestoreError::Calendar(_))
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn restored_hour_rolls_calendar_and_elapsed_days_independently() {
    for (start, expected) in [
        (
            [201.0, 1.0, 28.0, 23.5, 42.125, 60.0],
            Calendar {
                year: 201,
                month: 2,
                day: 1,
                hour: 0.5,
            },
        ),
        (
            [201.0, 11.0, 31.0, 23.5, 42.125, 60.0],
            Calendar {
                year: 202,
                month: 0,
                day: 1,
                hour: 0.5,
            },
        ),
    ] {
        let mut game = Game::new(calendar_order());
        game.world_mut()
            .restore_vanilla_globals(&calendar_save(start))
            .unwrap();
        game.tick_menu_mode(60.0, true);
        assert_eq!(game.world().clock.game_days, 42.125);
        game.tick(60.0);
        assert_eq!(game.world().calendar(), expected);
        assert!((game.world().clock.game_days - (42.125 + 1.0 / 24.0)).abs() < 1e-10);
        let elapsed = game.world().clock.game_days as f32;
        assert_eq!(game.world_mut().global_value(id(3, 0x904)), elapsed);
    }
}

#[test]
fn saved_zero_timescale_freezes_calendar_and_native_global_changes_drive_clock() {
    let mut game = Game::new(calendar_order());
    game.world_mut()
        .restore_vanilla_globals(&calendar_save([201.0, 7.0, 17.0, 12.0, 1.0, 0.0]))
        .unwrap();
    let calendar = game.world().calendar();
    game.tick(60.0);
    assert_eq!(game.world().calendar(), calendar);
    assert_eq!(game.world().clock.real_seconds, 60.0);
    game.world_mut().set_global(id(3, 0x905), 60.0);
    game.world_mut().set_global(id(3, 0x904), 4.0);
    game.tick(60.0);
    assert_eq!(game.world().calendar().hour, 13.0);
    assert!((game.world().clock.game_days - (4.0 + 1.0 / 24.0)).abs() < 1e-10);
}

#[test]
fn installed_sav_08_calendar_values_match_all_saved_globals_before_execution() {
    let (Some(_data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES not set, skipping calendar survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let order = Arc::new(order);
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    // CONTEXT: Require the corpus, all six globals per save, and no calendar failures.
    assert!(paths.len() >= 50);
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        let globals = save.globals().unwrap();
        let expected = CALENDAR_GLOBALS.map(|name| {
            let id = world.form(name).unwrap();
            globals
                .variables
                .iter()
                .find(|v| mapping.reference(v.form).unwrap() == Some(id))
                .unwrap()
                .value
        });
        world
            .restore_vanilla_globals(&save)
            .unwrap_or_else(|err| panic!("{}: {err}, {expected:?}", path.display()));
        assert_eq!(
            world.calendar(),
            Calendar {
                year: expected[0] as u32,
                month: expected[1] as u32,
                day: expected[2] as u32,
                hour: f64::from(expected[3])
            }
        );
        assert_eq!(world.clock.game_days, f64::from(expected[4]));
        assert_eq!(world.clock.timescale, f64::from(expected[5]));
        assert_eq!(world.clock.real_seconds, 0.0);
        assert!(world.pending.is_empty());
        for (name, value) in CALENDAR_GLOBALS.into_iter().zip(expected) {
            let id = world.form(name).unwrap();
            assert_eq!(world.global_value(id).to_bits(), value.to_bits());
        }
        let bytes = world.save_state();
        let mut restored = World::from_shared_order(Arc::clone(&order));
        restored.load_state(&bytes).unwrap();
        assert_eq!(restored.calendar(), world.calendar());
        assert_eq!(restored.clock, world.clock);
        assert_eq!(restored.state_hash(), world.state_hash());
        assert_eq!(restored.save_state(), bytes);
    }
    eprintln!(
        "SAV-08: {} saves, {} calendar globals restored before execution and persisted",
        paths.len(),
        paths.len() * 6
    );
}
