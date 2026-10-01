// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_59_form_and_force_ref_bindings_match_audited_classes() {
    let game = Game::new(LoadOrder::new());
    for name in [
        "GetFormID",
        "GetName",
        "GetType",
        "HasKeyword",
        "GetKeywords",
        "RegisterForSingleUpdate",
        "RegisterForUpdate",
        "RegisterForSingleUpdateGameTime",
        "RegisterForUpdateGameTime",
        "UnregisterForUpdate",
        "UnregisterForUpdateGameTime",
        "RegisterForSleep",
        "UnregisterForSleep",
        "RegisterForTrackedStatsEvent",
        "UnregisterForTrackedStatsEvent",
        "RegisterForAnimationEvent",
        "UnregisterForAnimationEvent",
        "RegisterForLOS",
        "RegisterForSingleLOSGain",
        "RegisterForSingleLOSLost",
        "UnregisterForLOS",
    ] {
        assert!(game.vm.natives.has_method("Form", name), "Form.{name}");
    }
    // ReferenceAlias inherits Alias, so the native is registered on Alias.
    assert!(game.vm.natives.has_method("Alias", "ForceRefTo"));
    assert!(!game.vm.natives.has_global("Alias", "ForceRefTo"));
}
