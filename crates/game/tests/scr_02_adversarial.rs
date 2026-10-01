// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-02 binding-kind regression checks for the audited native inventory.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_02_audited_actor_and_effect_bindings_are_methods_only() {
    let game = Game::new(LoadOrder::new());
    for (class, methods) in [
        (
            "ActiveMagicEffect",
            &[
                "AddInventoryEventFilter",
                "RemoveInventoryEventFilter",
                "RemoveAllInventoryEventFilters",
            ][..],
        ),
        (
            "Actor",
            &[
                "IsChild",
                "IsCommandedActor",
                "IsOnMount",
                "IsTrespassing",
                "IsDetectedBy",
                "GetSitState",
                "GetFlyingState",
                "SetEssential",
                "SetVehicle",
                "SetLookAt",
                "ClearLookAt",
                "SetHeadTracking",
                "SetExpressionOverride",
                "ClearExpressionOverride",
                "ShowGiftMenu",
                "ShowBarterMenu",
                "OpenInventory",
                "SendTrespassAlarm",
                "StopCombatAlarm",
                "ClearForcedMovement",
                "ClearKeepOffsetFromActor",
                "KeepOffsetFromActor",
                "SetForcedLandingMarker",
                "GetEquippedWeapon",
                "GetEquippedShield",
                "GetEquippedSpell",
                "GetVoiceRecoveryTime",
                "PlayIdleWithTarget",
                "Dismount",
                "Trespass",
            ][..],
        ),
    ] {
        for method in methods {
            assert!(
                game.vm.natives.has_method(class, method),
                "{class}.{method}"
            );
            assert!(
                !game.vm.natives.has_global(class, method),
                "{class}.{method} must be a method"
            );
        }
    }
}
