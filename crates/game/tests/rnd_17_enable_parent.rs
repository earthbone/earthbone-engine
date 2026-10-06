// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 effective enable state follows UESP REFR XESP and xEdit wbXESP.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::{Game, World};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode, Value,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(object: u32, flags: u32, parent: Option<(u32, u32)>) -> Record {
    let mut record = Record::new(tag::REFR, object)
        .with_flags(flags)
        .sub(b"NAME", 0x800_u32.to_le_bytes())
        .sub(b"DATA", [0; 24]);
    if let Some((parent, flags)) = parent {
        record = record.sub(
            b"XESP",
            [parent.to_le_bytes(), flags.to_le_bytes()].concat(),
        );
    }
    record
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800));
    for record in [
        reference(0x900, flags::INITIALLY_DISABLED, None),
        reference(0x901, 0, Some((0x900, 0))),
        reference(0x902, flags::INITIALLY_DISABLED, Some((0x901, 1))),
        reference(0x903, 0, Some((0x902, 1))),
        reference(0x904, flags::DELETED, Some((0x900, 1))),
        reference(0x905, 0, Some((0x999, 1))),
        reference(0x906, 0, Some((0x907, 0))),
        reference(0x907, 0, Some((0x906, 1))),
        reference(0x908, 0, Some((0x900, 0xFFFF_FFFE))),
        reference(0x909, flags::INITIALLY_DISABLED, Some((0x902, 0))),
        reference(0x90A, 0, None).sub(b"XESP", [0; 3]),
        reference(0x90B, 0, Some((0, 0))),
    ] {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, target: GlobalFormId, method: &str, returns: &str) -> Value {
    let script = format!("Rnd17{method}");
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            FunctionBuilder::new("Run", returns)
                .global()
                .param("target", "ObjectReference")
                .local("result", "Var")
                .ins(
                    Opcode::CallMethod,
                    vec![s(method), ident("target"), ident("result")],
                )
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(target);
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::Object(object)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn rnd_17_initial_flags_parent_chains_opposites_and_pop_in() {
    let game = game();
    let world = game.world();
    for (object, expected) in [
        (0x900, false),
        (0x901, false),
        (0x902, true),
        (0x903, false),
        (0x908, false),
        (0x909, true),
    ] {
        assert_eq!(
            world.reference_enabled(id(object)),
            expected,
            "{object:06X}"
        );
        assert_eq!(world.ref_placement(id(object)).unwrap().visible, expected);
        assert!(
            world.reference_ref(id(object)).is_none(),
            "query must not construct runtime state"
        );
    }
}

#[test]
fn rnd_17_papyrus_enable_disable_updates_descendants_and_queries() {
    let mut game = game();
    call(&mut game, id(0x900), "Enable", "None");
    assert!(game.world().reference_enabled(id(0x901)));
    assert!(!game.world().reference_enabled(id(0x902)));
    assert_eq!(
        call(&mut game, id(0x901), "IsEnabled", "Bool"),
        Value::Bool(true)
    );
    call(&mut game, id(0x900), "Disable", "None");
    assert!(!game.world().reference_enabled(id(0x901)));
    assert!(game.world().reference_enabled(id(0x902)));
    assert_eq!(
        call(&mut game, id(0x901), "IsDisabled", "Bool"),
        Value::Bool(true)
    );
}

#[test]
fn rnd_17_missing_deleted_cyclic_and_truncated_links_stay_hidden() {
    let game = game();
    for object in [0x904, 0x905, 0x906, 0x907, 0x90A, 0x90B, 0x999] {
        assert!(!game.world().reference_enabled(id(object)), "{object:06X}");
    }
    game.world_mut().reference(id(0x900)).unwrap().deleted = true;
    assert!(!game.world().reference_enabled(id(0x902)));
}

#[test]
fn rnd_17_parent_state_survives_save_load_without_new_saved_fields() {
    let game = game();
    game.world_mut().set_reference_enabled(id(0x900), true);
    let bytes = game.world().save_state();
    let restored = self::game();
    restored.world_mut().load_state(&bytes).unwrap();
    assert!(restored.world().reference_enabled(id(0x901)));
    assert!(!restored.world().reference_enabled(id(0x902)));
}

#[test]
fn rnd_17_winning_parent_links_resolve_in_the_override_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(reference(0x900, 0, None));
    base.push_top(reference(0x901, 0, Some((0x900, 0))));
    let mut update = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    update.push_top(reference(0x0100_0900, flags::INITIALLY_DISABLED, None));
    update.push_top(reference(0x901, 0, Some((0x0100_0900, 0))));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Update.esm", update).unwrap();
    assert!(!World::authored_reference_enabled(&order, id(0x901)));
    let mut world = World::new(order);
    world.set_reference_enabled(
        GlobalFormId {
            file: 1,
            object: 0x900,
        },
        true,
    );
    assert!(world.reference_enabled(id(0x901)));
}
