// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Registry and behavior checks for the GetID and GetName contracts in the
//! Creation Kit wiki's Form, Quest, and Alias script pages.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::quest;
use soe_game::{Game, Handle};
use soe_papyrus::Opcode::CallMethod;
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FORM: u32 = 0x800;
const QUEST: u32 = 0x801;
const ALIAS: u32 = 23;
const ACTOR: u32 = 0x802;
const REFERENCE: u32 = 0x803;
const UNNAMED_FORM: u32 = 0x804;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"MISC"), FORM)
            .zsub(T(b"EDID"), "AuditForm")
            .zsub(T(b"FULL"), "Name column"),
    );
    plugin.push_top(Record::new(T(b"MISC"), UNNAMED_FORM).zsub(T(b"EDID"), "UnnamedForm"));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "AuditActor")
            .sub(T(b"NAME"), FORM.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), REFERENCE)
            .zsub(T(b"EDID"), "AuditReference")
            .sub(T(b"NAME"), FORM.to_le_bytes()),
    );
    let quest = quest::build_record(QUEST, "AuditQuest", 0);
    plugin.push_top(quest::with_alias(quest, ALIAS, "UniqueAliasName", None));
    let bytes = plugin.to_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut order = LoadOrder::new();
    order.add("Scr61.esm", plugin).unwrap();
    Game::new(order)
}

fn call_with_args(game: &mut Game, target: &Value, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr61Probe{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, argument) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        let kind = match argument {
            Value::Int(_) => "Int",
            Value::Str(_) => "String",
            _ => panic!("unsupported SCR-61 probe argument"),
        };
        function = function.param(&parameter, kind);
        operands.push(ident(&parameter));
    }
    let function = function
        .ins(CallMethod, operands)
        .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let mut call_args = vec![target.clone()];
    call_args.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &call_args).unwrap();
    game.run_until_idle(32);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn call(game: &mut Game, target: &Value, name: &str) -> Value {
    call_with_args(game, target, name, &[])
}

/// The registry-derived inventory contains one binding for each documented
/// Form, Quest, and Alias method contract.
#[test]
fn scr_61_getid_getname_registry_inventory_matches_ck_bindings() {
    let game = game();
    let expected = [
        ("Form", "GetName"),
        ("Quest", "GetID"),
        ("Alias", "GetID"),
        ("Alias", "GetName"),
    ];
    for (class, method) in expected {
        assert!(
            game.vm.natives.has_method(class, method),
            "CK contract binding {class}.{method} is missing from the registry"
        );
    }
}

/// CK's Form.GetName returns the name-column text, not the EditorID.
#[test]
fn scr_61_form_getname_returns_name_column_text() {
    let mut game = game();
    let form = game.form("AuditForm").unwrap();
    let target = Value::Object(game.object_of_form(form));

    assert_eq!(
        call(&mut game, &target, "GetName"),
        Value::string("Name column"),
        "Form.GetName follows the CK name-column contract"
    );
}

/// CK notes that forms without a name do not provide name-column text.
#[test]
fn scr_61_form_getname_returns_empty_for_unnamed_form() {
    let mut game = game();
    let form = game.form("UnnamedForm").unwrap();
    let target = Value::Object(game.object_of_form(form));

    assert_eq!(
        call(&mut game, &target, "GetName"),
        Value::string(""),
        "an unnamed form has no name-column text"
    );
}

/// CK's SKSE Form.GetName page excludes actor and object-reference instances.
#[test]
fn scr_61_form_getname_does_not_resolve_actor_or_reference_base_names() {
    let mut game = game();
    for editor_id in ["AuditActor", "AuditReference"] {
        let form = game.form(editor_id).unwrap();
        let target = Value::Object(game.object_of_form(form));
        assert_eq!(
            call(&mut game, &target, "GetName"),
            Value::string(""),
            "Form.GetName does not return a base name for {editor_id}"
        );
    }
}

/// CK's Quest.GetID returns the EditorID, while Alias.GetID and GetName return
/// its numeric ID and unique name used by GetAliasByName.
#[test]
fn scr_61_quest_and_alias_getid_getname_follow_ck_contracts() {
    let mut game = game();
    let quest = game.form("AuditQuest").unwrap();
    let quest_target = Value::Object(game.object_of_form(quest));
    let alias_target = Value::Object(game.object_of(Handle::Alias { quest, id: ALIAS }));

    assert_eq!(
        call(&mut game, &quest_target, "GetID"),
        Value::string("AuditQuest"),
        "Quest.GetID returns the quest EditorID"
    );
    assert_eq!(
        call(&mut game, &alias_target, "GetID"),
        Value::Int(ALIAS as i32),
        "Alias.GetID returns the alias ID"
    );
    assert_eq!(
        call(&mut game, &alias_target, "GetName"),
        Value::string("UniqueAliasName"),
        "Alias.GetName returns the unique alias name"
    );
    assert_eq!(
        call_with_args(
            &mut game,
            &quest_target,
            "GetAlias",
            &[Value::Int(ALIAS as i32)]
        ),
        alias_target,
        "Quest.GetAlias accepts the ID returned by Alias.GetID"
    );
    assert_eq!(
        call_with_args(
            &mut game,
            &quest_target,
            "GetAliasByName",
            &[Value::string("UniqueAliasName")]
        ),
        alias_target,
        "Quest.GetAliasByName accepts the name returned by Alias.GetName"
    );
}
