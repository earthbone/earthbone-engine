// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-57 registry inventory for the Debug and EffectShader contract audit.

use std::cell::RefCell;
use std::rc::Rc;

use soe_esm::LoadOrder;
use soe_game::{natives, World};
use soe_papyrus::Vm;

const DEBUG_GLOBALS: &[&str] = &[
    "Trace",
    "TraceUser",
    "TraceConditional",
    "TraceStack",
    "Notification",
    "MessageBox",
    "SendAnimationEvent",
    "OpenUserLog",
    "QuitGame",
    "CenterOnCell",
    "CenterOnCellAndWait",
    "CloseUserLog",
    "DumpAliasData",
    "DebugChannelNotify",
    "SetGodMode",
    "ToggleAI",
    "ToggleCollisions",
    "StartScriptProfiling",
    "StopScriptProfiling",
    "StartStackProfiling",
    "StopStackProfiling",
    "TakeScreenshot",
    "SetFootIK",
    "ShowRefPosition",
    "PlayerMoveToAndWait",
    "MoveToCell",
];

const EFFECT_SHADER_METHODS: &[&str] = &["Play", "Stop"];

/// Every Debug global in the audit table is present in the native registry.
#[test]
fn scr_57_registry_contains_every_audited_binding() {
    let mut vm = Vm::new();
    let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
    natives::register(&mut vm, &world);

    for name in DEBUG_GLOBALS {
        assert!(
            vm.natives.has_global("Debug", name),
            "Debug.{name} is missing from the registry"
        );
    }
    for name in EFFECT_SHADER_METHODS {
        assert!(
            vm.natives.has_method("EffectShader", name),
            "EffectShader.{name} is missing from the registry"
        );
    }
    assert_eq!(DEBUG_GLOBALS.len(), 26, "Debug audit inventory changed");
    assert_eq!(
        EFFECT_SHADER_METHODS.len(),
        2,
        "EffectShader audit inventory changed"
    );
}
