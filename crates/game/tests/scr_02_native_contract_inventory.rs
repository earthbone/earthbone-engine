// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Registry-derived inventory checks for SCR-02. The expected method names
//! were reconciled with the ActiveMagicEffect PSC and Actor CK pages listed
//! in `docs/game.md`.

use soe_esm::LoadOrder;
use soe_game::Game;

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
