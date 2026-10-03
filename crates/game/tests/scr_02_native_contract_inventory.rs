// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Registry-derived inventory checks against the SCR-02 CK contract manifest.

use soe_esm::LoadOrder;
use soe_game::Game;

const CK_CONTRACTS: &str = include_str!("fixtures/scr_02_ck_contracts.txt");

fn ck_contract_names() -> Vec<(String, String)> {
    CK_CONTRACTS
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '|');
            let class = fields.next()?;
            let source = fields.next()?;
            let declaration = fields.next()?;
            if class.starts_with('#') {
                return None;
            }
            if source.is_empty() {
                return None;
            }
            let name = declaration
                .split_once('(')?
                .0
                .split_whitespace()
                .last()?
                .to_ascii_lowercase();
            Some((class.to_ascii_lowercase(), name))
        })
        .collect()
}

#[test]
fn scr_02_active_magic_effect_methods_match_the_audited_registry_inventory() {
    let game = Game::new(LoadOrder::new());
    let methods = [
        "GetBaseObject",
        "GetTargetActor",
        "GetCasterActor",
        "Dispel",
        "AddInventoryEventFilter",
        "RemoveInventoryEventFilter",
        "RemoveAllInventoryEventFilters",
        "RegisterForSingleUpdate",
        "RegisterForUpdate",
        "RegisterForSingleUpdateGameTime",
        "RegisterForUpdateGameTime",
        "UnregisterForUpdate",
        "UnregisterForUpdateGameTime",
        "RegisterForAnimationEvent",
        "UnregisterForAnimationEvent",
    ];

    for method in methods {
        assert!(
            game.vm.natives.has_method("ActiveMagicEffect", method),
            "ActiveMagicEffect.{method} is listed in the audit but absent from the registry"
        );
    }
    assert!(
        !game
            .vm
            .natives
            .has_global("ActiveMagicEffect", "GetBaseObject"),
        "ActiveMagicEffect.GetBaseObject is a method, not a global binding"
    );
    assert!(
        !game.vm.natives.has_global("Actor", "GetActorValue"),
        "Actor.GetActorValue is a method, not a global binding"
    );
}

#[test]
fn scr_02_every_actor_and_effect_binding_has_a_ck_contract_comparison() {
    let game = Game::new(LoadOrder::new());
    let contracts = ck_contract_names();
    let audited_classes = ["actor", "activemagiceffect"];

    assert!(
        game.vm.natives.has_method("Actor", "AllowPCDialogue"),
        "CK declares Actor.AllowPCDialogue"
    );
    assert!(
        !game.vm.natives.has_method("Actor", "SetAllowPCDialogue"),
        "Actor.SetAllowPCDialogue is not a CK binding"
    );

    for class in audited_classes {
        let methods: Vec<_> = game
            .vm
            .natives
            .method_bindings()
            .into_iter()
            .filter(|(binding_class, _)| binding_class == class)
            .collect();
        assert!(!methods.is_empty(), "{class} has no registry methods");

        for (binding_class, method) in methods {
            assert!(
                contracts.iter().any(|(contract_class, contract_name)| {
                    contract_class == &binding_class && contract_name == &method
                }),
                "registry method {binding_class}.{method} has no CK or PSC declaration and contract comparison"
            );
        }

        assert!(
            !game
                .vm
                .natives
                .global_bindings()
                .iter()
                .any(|(binding_class, _)| binding_class == class),
            "{class} has no CK global bindings"
        );
    }
}

#[test]
fn scr_02_actor_contract_findings_are_registered_to_their_owning_rows() {
    let game = Game::new(LoadOrder::new());
    let findings = [
        "GetCombatState",
        "GetEquippedWeapon",
        "GetEquippedShield",
        "HasLOS",
        "PathToReference",
        "IsFlying",
        "GetSleepState",
        "GetSitState",
        "GetFlyingState",
        "SetEssential",
        "EquipItem",
    ];

    for method in findings {
        assert!(
            game.vm.natives.has_method("Actor", method),
            "Actor.{method} is listed in the audit but absent from the registry"
        );
    }
}
