// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-19 health workloads using live timers and notified inventory mutations.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{f, i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{ObjectId, Opcode};

const HOLDER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const ITEM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};

fn script() -> Vec<u8> {
    let mut builder = ScriptBuilder::new("Health", "Form");
    for (function, native) in [
        ("Repeat", "RegisterForUpdate"),
        ("Single", "RegisterForSingleUpdate"),
        ("RepeatGame", "RegisterForUpdateGameTime"),
        ("SingleGame", "RegisterForSingleUpdateGameTime"),
    ] {
        builder = builder.function(
            FunctionBuilder::new(function, "None")
                .local("n", "Int")
                .local("more", "Bool")
                .ins(Opcode::Assign, vec![ident("n"), i(0)])
                .ins(
                    Opcode::CallMethod,
                    vec![ident(native), ident("self"), ident("::nonevar"), f(1.0)],
                )
                .ins(Opcode::IAdd, vec![ident("n"), ident("n"), i(1)])
                .ins(Opcode::CmpLt, vec![ident("more"), ident("n"), i(1000)])
                .ins(Opcode::JmpT, vec![ident("more"), i(-3)])
                .ins(Opcode::Return, vec![])
                .build(),
        );
    }
    for name in ["OnUpdate", "OnUpdateGameTime", "OnItemAdded"] {
        builder = builder.function(
            FunctionBuilder::new(name, "None")
                .ins(Opcode::Return, vec![])
                .build(),
        );
    }
    builder.bytes()
}

fn fixture() -> (Game, ObjectId) {
    let mut plugin = Plugin::new(FileHeader::default());
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Health")],
        ..Vmad::default()
    }
    .write(Tag::new(b"CONT"));
    plugin.push_top(Record::new(Tag::new(b"CONT"), 0x802).sub(Tag::new(b"VMAD"), vmad));
    plugin.push_top(Record::new(Tag::new(b"MISC"), ITEM.object));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), HOLDER.object)
            .sub(Tag::new(b"NAME"), 0x802u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Health.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(&script()).unwrap();
    game.bind();
    let object = game.object_of_form(HOLDER);
    game.vm.attach(object, "Health").unwrap();
    (game, object)
}

#[test]
fn prf_19_one_thousand_registration_calls_replace_each_mode_and_report_per_script() {
    let (mut game, object) = fixture();
    for function in ["Repeat", "Single", "RepeatGame", "SingleGame"] {
        game.vm.call(object, function, &[]).unwrap();
        assert!(game.run_until_idle(1));
    }
    let report = game.papyrus_health();
    let row = &report.scripts["health"];
    assert_eq!(
        row.updates,
        soe_trace::UpdateRegistrations {
            repeating: 1,
            single: 1,
            repeating_game_time: 1,
            single_game_time: 1,
        }
    );
    assert_eq!(report.total.updates, row.updates);
    assert_eq!(report.plugins["Health.esp"]["health"], *row);
    assert_eq!(game.vm.natives.calls["form.registerforupdate"], 1000);
    game.world_mut().unregister_update(object, false);
    assert_eq!(game.papyrus_health().total.updates.repeating, 0);
    assert_eq!(game.papyrus_health().total.updates.single, 0);
    assert_eq!(game.papyrus_health().total.updates.repeating_game_time, 1);
}

#[test]
fn prf_19_one_thousand_registered_instances_report_one_thousand_live_updates() {
    let (mut game, _) = fixture();
    for _ in 0..1000 {
        let object = game.vm.create_object_of("instance", "Form");
        game.vm.attach(object, "Health").unwrap();
        game.world_mut().register_update(object, 10.0, true, false);
    }
    assert_eq!(
        game.papyrus_health().scripts["health"].updates.repeating,
        1000
    );
    assert_eq!(game.papyrus_health().total.updates.repeating, 1000);
}

#[test]
fn prf_19_five_hundred_item_additions_report_pending_and_vm_queue_and_event_peak() {
    let (mut game, _) = fixture();
    game.world_mut().pending.clear();
    let world = game.shared_world();
    for _ in 0..500 {
        let vm = &mut game.vm;
        assert!(world
            .borrow_mut()
            .add_item_notified(HOLDER, ITEM, 1, &mut |name, class| vm
                .create_object_of(name, class)));
    }
    assert_eq!(
        game.papyrus_health().total.events["onitemadded"].queued,
        500
    );
    assert_eq!(
        game.papyrus_health().scripts["health"].events["onitemadded"].queued,
        500
    );
    assert_eq!(game.pump(), 500);
    let report = game.papyrus_health();
    assert_eq!(report.total.events["onitemadded"].queued, 500);
    assert_eq!(report.total.events["onitemadded"].peak_per_second, 500);
    assert_eq!(
        report.plugins["Health.esp"]["health"].events["onitemadded"].peak_per_second,
        500
    );
    game.vm.run(1000);
    assert_eq!(game.papyrus_health().total.events["onitemadded"].queued, 0);
}

#[test]
fn prf_19_due_single_updates_expire_and_menu_mode_keeps_registrations() {
    let (mut game, object) = fixture();
    for (repeat, game_time) in [(true, false), (false, false), (true, true), (false, true)] {
        game.world_mut()
            .register_update(object, 0.1, repeat, game_time);
    }
    game.tick_menu_mode(3600.0, true);
    assert_eq!(game.papyrus_health().scripts["health"].updates.single, 1);
    assert_eq!(
        game.papyrus_health().scripts["health"]
            .updates
            .single_game_time,
        1
    );
    game.tick(1.0);
    assert_eq!(game.papyrus_health().scripts["health"].updates.single, 0);
    game.tick(3600.0);
    let updates = &game.papyrus_health().scripts["health"].updates;
    assert_eq!(
        (
            updates.repeating,
            updates.single,
            updates.repeating_game_time,
            updates.single_game_time
        ),
        (1, 0, 1, 0)
    );
}

#[test]
fn prf_19_installed_form_registrations_use_the_winning_plugin() {
    let Some(mut game) = crate::common::installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    game.load_script(&script()).unwrap();
    let player = game.player();
    let object = game.object_of_form(player);
    let base = game.world_mut().reference(player).unwrap().base.unwrap();
    let record = game.world().order().winner_ref(base).unwrap();
    let plugin = game.world().order().files()[record.file].name.clone();
    game.vm.attach(object, "Health").unwrap();
    game.vm.call(object, "Repeat", &[]).unwrap();
    assert!(game.run_until_idle(1));
    assert_eq!(game.papyrus_health().scripts["health"].updates.repeating, 1);
    assert_eq!(
        game.papyrus_health().plugins[&plugin]["health"]
            .updates
            .repeating,
        1
    );
}

#[test]
fn prf_19_update_fanout_counts_each_script_but_shared_timer_once() {
    let (mut game, object) = fixture();
    game.load_script(
        &ScriptBuilder::new("Second", "Form")
            .function(
                FunctionBuilder::new("OnUpdate", "None")
                    .ins(Opcode::Return, vec![])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    game.vm.attach(object, "Second").unwrap();
    game.world_mut().register_update(object, 1.0, true, false);
    game.world_mut().advance_clock(1.0);
    game.world_mut().fire_timers();
    assert_eq!(game.papyrus_health().total.updates.repeating, 1);
    assert_eq!(game.papyrus_health().scripts["health"].updates.repeating, 1);
    assert_eq!(game.papyrus_health().scripts["second"].updates.repeating, 1);
    game.pump();
    assert_eq!(game.papyrus_health().total.events["onupdate"].queued, 2);
    assert_eq!(
        game.papyrus_health().total.events["onupdate"].peak_per_second,
        2
    );
}

#[test]
fn prf_19_quest_stop_removes_quest_and_alias_timers() {
    let quest = GlobalFormId {
        file: 0,
        object: 0x900,
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(soe_game::quest::build_record(
        quest.object,
        "HealthQuest",
        0,
    ));
    let mut order = LoadOrder::new();
    order.add("Health.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.start_quest(quest);
    let owner = game.object_of(soe_game::Handle::Form(quest));
    let alias = game.object_of(soe_game::Handle::Alias { quest, id: 0 });
    for object in [owner, alias] {
        game.world_mut().register_update(object, 1.0, true, false);
        game.world_mut().register_update(object, 1.0, false, true);
    }
    assert_eq!(game.papyrus_health().total.updates.repeating, 2);
    assert!(game.world_mut().stop_quest(quest));
    assert_eq!(
        game.papyrus_health().total.updates,
        soe_trace::UpdateRegistrations::default()
    );
}

#[test]
fn prf_19_script_and_plugin_peak_does_not_add_peaks_from_disjoint_seconds() {
    let (mut game, first) = fixture();
    let second = game.vm.create_object_of("second", "Form");
    game.vm.attach(second, "Health").unwrap();
    for object in [first, second] {
        game.vm.set_health_owner(object, "Health", "Health.esp");
        for _ in 0..250 {
            game.vm.send_event(object, "OnItemAdded", &[]);
        }
        game.vm.run(1000);
        game.vm.advance(2.0);
    }
    let report = game.papyrus_health();
    assert_eq!(report.total.events["onitemadded"].peak_per_second, 250);
    assert_eq!(
        report.scripts["health"].events["onitemadded"].peak_per_second,
        250
    );
    assert_eq!(
        report.plugins["Health.esp"]["health"].events["onitemadded"].peak_per_second,
        250
    );
}

#[test]
fn prf_19_inherited_handler_keeps_its_attachment_plugin() {
    let (mut game, _) = fixture();
    game.load_script(&ScriptBuilder::new("Derived", "Health").bytes())
        .unwrap();
    let object = game.vm.create_object_of("derived", "Form");
    game.vm.attach(object, "Derived").unwrap();
    game.vm.set_health_owner(object, "Derived", "Derived.esp");
    game.world_mut().pending.push(soe_game::Pending::Event {
        object,
        name: "OnItemAdded".into(),
        args: vec![],
    });
    game.pump();
    let event = &game.papyrus_health().plugins["Derived.esp"]["health"].events["onitemadded"];
    assert_eq!((event.queued, event.peak_per_second), (1, 1));
}
