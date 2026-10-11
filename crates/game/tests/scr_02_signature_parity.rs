// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Full source-signature coverage for the SCR-02 native inventory.

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Value;

const CONTRACTS: &str = include_str!("fixtures/scr_02_ck_contracts.txt");

fn contract_name(declaration: &str) -> Option<&str> {
    let head = declaration.split_once('(')?.0;
    head.split_whitespace().last()
}

fn assert_complete_signature(declaration: &str, class: &str, method: &str) {
    let signature = declaration
        .split_once('(')
        .unwrap_or_else(|| panic!("{class}.{method} has no parameter list: {declaration}"));
    let args = signature
        .1
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("{class}.{method} has no closing parenthesis: {declaration}"));
    let head: Vec<_> = signature.0.split_whitespace().collect();
    assert_eq!(
        head.last()
            .is_some_and(|declared| declared.eq_ignore_ascii_case(method)),
        true,
        "signature method name for {class}.{method}"
    );
    let return_type = match head.len() {
        1 => "None",
        2 if head[0].eq_ignore_ascii_case("Function") => "None",
        2 => head[0],
        3 if head[1].eq_ignore_ascii_case("Function") => head[0],
        _ => panic!("unrecognized return declaration for {class}.{method}: {declaration}"),
    };
    assert!(
        !return_type.is_empty()
            && return_type
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
        "{class}.{method} return type is missing or malformed"
    );

    for parameter in args
        .split(',')
        .filter(|parameter| !parameter.trim().is_empty())
    {
        let parameter = parameter.trim();
        let mut parts = parameter.split('=');
        let declaration = parts.next().unwrap().trim();
        if let Some(default) = parts.next() {
            assert!(
                !default.trim().is_empty() && parts.next().is_none(),
                "{class}.{method} optional parameter has an invalid default: {parameter}"
            );
        }
        let parameter = declaration;
        let tokens: Vec<_> = parameter.split_whitespace().collect();
        assert_eq!(
            tokens.len(),
            2,
            "{class}.{method} parameter needs a type and name: {parameter}"
        );
        assert!(
            tokens[0]
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_'),
            "{class}.{method} parameter type is missing: {parameter}"
        );
    }
}

#[test]
fn scr_02_every_registered_binding_has_a_full_source_signature() {
    let game = Game::new(LoadOrder::new());
    let contracts: Vec<_> = CONTRACTS
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '|');
            let class = fields.next()?;
            let source = fields.next()?;
            let declaration = fields.next()?;
            (!class.starts_with('#') && !source.is_empty()).then_some((class, source, declaration))
        })
        .collect();

    for (class, method) in game.vm.natives.method_bindings() {
        if class != "actor" && class != "activemagiceffect" {
            continue;
        }
        let matches: Vec<_> = contracts
            .iter()
            .filter(|(contract_class, _, declaration)| {
                contract_class.eq_ignore_ascii_case(&class)
                    && contract_name(declaration)
                        .is_some_and(|name| name.eq_ignore_ascii_case(&method))
            })
            .collect();
        assert!(
            !matches.is_empty(),
            "{class}.{method} has no CK or shipped PSC signature"
        );
        for (_, _, declaration) in matches {
            assert_complete_signature(declaration, &class, &method);
        }
    }

    assert!(
        !game
            .vm
            .natives
            .global_bindings()
            .iter()
            .any(|(class, _)| class == "actor" || class == "activemagiceffect"),
        "Actor and ActiveMagicEffect have no declared global bindings"
    );
}

#[test]
fn scr_02_enable_ai_uses_the_vanilla_true_default() {
    let mut game = crate::natives_dlc::game();
    let actor = crate::natives_dlc::obj(&mut game, "VampireRef");
    let actor_id = crate::natives_dlc::form(&game, "VampireRef");

    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[]),
        Value::None
    );
    assert!(!game.world().actor(actor_id).unwrap().ai_disabled);

    assert_eq!(
        crate::natives_dlc::call(&mut game, &actor, "EnableAI", &[Value::Bool(false)]),
        Value::None
    );
    assert!(game.world().actor(actor_id).unwrap().ai_disabled);
}

#[test]
fn scr_02_allow_flying_methods_use_ck_defaults_and_none_returns() {
    let expected = [
        "Actor|ck-wiki/SetAllowFlying_-_Actor.wikitext|None SetAllowFlying(Bool abAllowed = true)",
        "Actor|ck-wiki/SetAllowFlyingEx_-_Actor.wikitext|None SetAllowFlyingEx(Bool abAllowed = true, Bool abAllowCrash = true, Bool abAllowSearch = false)",
    ];
    for declaration in expected {
        assert!(
            CONTRACTS.lines().any(|line| line == declaration),
            "missing CK signature with defaults: {declaration}"
        );
    }

    let mut game = crate::natives_dlc::game();
    let actor = crate::natives_dlc::obj(&mut game, "VampireRef");
    let actor_id = crate::natives_dlc::form(&game, "VampireRef");

    for method in ["SetAllowFlying", "SetAllowFlyingEx"] {
        assert_eq!(
            crate::natives_dlc::call(&mut game, &actor, method, &[]),
            Value::None,
            "{method} returns None when using its default arguments"
        );
        assert!(game.world().actor(actor_id).unwrap().allow_flying);

        assert_eq!(
            crate::natives_dlc::call(&mut game, &actor, method, &[Value::Bool(false)]),
            Value::None,
            "{method} returns None with an explicit argument"
        );
        assert!(!game.world().actor(actor_id).unwrap().allow_flying);
    }
}
