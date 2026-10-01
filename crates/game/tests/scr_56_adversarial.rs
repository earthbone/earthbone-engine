// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{natives, Game, World};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{Value, Vm};

#[test]
fn cell_owner_contract_inventory_covers_ck_typed_setters_and_clear_alias() {
    let audit = include_str!("../../../docs/reference-audit.md");
    for contract in [
        "`Cell.SetActorOwner`",
        "`Cell.SetFactionOwner`",
        "`Alias.Clear`",
    ] {
        assert!(audit.contains(contract), "missing audit for {contract}");
    }
    assert!(audit.contains("SetActorOwner_-_Cell.wikitext"));
    assert!(audit.contains("Cell.psc"));
    assert!(audit.contains("errors for a non-optional alias"));
}

#[test]
fn cell_and_clear_audit_covers_every_registered_binding() {
    let mut vm = Vm::new();
    let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
    natives::register(&mut vm, &world);

    let mut registered: Vec<_> = vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, name)| *class == "cell" || *name == "clear")
        .collect();
    registered.sort_unstable();
    assert_eq!(
        registered,
        [
            ("alias".to_owned(), "clear".to_owned()),
            ("cell".to_owned(), "getactorowner".to_owned()),
            ("cell".to_owned(), "getfactionowner".to_owned()),
            ("cell".to_owned(), "isattached".to_owned()),
            ("cell".to_owned(), "isinterior".to_owned()),
            ("cell".to_owned(), "reset".to_owned()),
            ("cell".to_owned(), "setactorowner".to_owned()),
            ("cell".to_owned(), "setfactionowner".to_owned()),
            ("cell".to_owned(), "setpublic".to_owned()),
        ]
    );
    assert!(vm
        .natives
        .global_bindings()
        .into_iter()
        .all(|(class, name)| class != "cell" && name != "clear"));

    let audit = include_str!("../../../docs/reference-audit.md");
    for binding in [
        "Cell.GetActorOwner",
        "Cell.GetFactionOwner",
        "Cell.IsInterior",
        "Cell.IsAttached",
        "Cell.Reset",
        "Cell.SetActorOwner",
        "Cell.SetFactionOwner",
        "Cell.SetPublic",
        "Alias.Clear",
    ] {
        assert!(audit.contains(&format!("`{binding}`")), "missing {binding}");
    }
    for source in [
        "GetActorOwner_-_Cell.wikitext",
        "Cell_Script.wikitext",
        "SetActorOwner_-_Cell.wikitext",
        "Clear_-_ReferenceAlias.wikitext",
    ] {
        assert!(audit.contains(source), "missing source {source}");
    }
    assert!(audit.contains("SCR-19 owns the live attachment query"));
    assert!(audit.contains("SCR-19 owns cell reset behavior"));
    assert!(audit.contains("SCR-68 owns both gaps"));
    let backlog = include_str!("../../../BACKLOG.csv");
    for row in ["SCR-19", "SCR-68"] {
        assert!(
            backlog.contains(&format!("\"{row}\"")),
            "missing owner row {row}"
        );
    }
}

#[test]
fn invalid_cell_owner_type_does_not_clear_the_existing_owner() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 1));
    plugin.push_top(Record::new(Tag::new(b"FACT"), 2));
    plugin.push_top(Record::new(Tag::new(b"CELL"), 3).sub(Tag::new(b"XOWN"), 1_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order
        .add("scr56-breaker.esp", plugin)
        .expect("plugin loads");
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Scr56CellProbe", "Cell")
        .function(
            FunctionBuilder::new("Run", "None")
                .param("owner", "Form")
                .local("result", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SetActorOwner"),
                        ident("self"),
                        ident("result"),
                        ident("owner"),
                    ],
                )
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).expect("script loads");
    let cell = game.object_of_form(GlobalFormId { file: 0, object: 3 });
    let wrong_owner = game.object_of_form(GlobalFormId { file: 0, object: 2 });
    game.vm
        .attach(cell, "Scr56CellProbe")
        .expect("attach script");
    let thread = game
        .vm
        .call(cell, "Run", &[Value::Object(wrong_owner)])
        .expect("call starts");
    game.run_until_idle(16);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert_eq!(
        game.world_mut()
            .cell_state(GlobalFormId { file: 0, object: 3 })
            .unwrap()
            .owner,
        Some(GlobalFormId { file: 0, object: 1 }),
        "an argument outside ActorBase type must leave ownership unchanged"
    );
}
