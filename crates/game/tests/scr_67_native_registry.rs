// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

//! Registry inventory for SCR-67. The names are cross-checked against the
//! CK pages listed in `docs/reference-audit.md`.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_67_object_reference_method_inventory_matches_registry() {
    let game = Game::new(LoadOrder::new());
    let methods = [
        "Enable",
        "EnableNoWait",
        "Disable",
        "DisableNoWait",
        "IsEnabled",
        "IsDisabled",
        "Delete",
        "IsDeleted",
        "GetBaseObject",
        "GetParentCell",
        "GetWorldSpace",
        "IsInInterior",
        "GetPositionX",
        "GetPositionY",
        "GetPositionZ",
        "GetAngleX",
        "GetAngleY",
        "GetAngleZ",
        "SetPosition",
        "SetAngle",
        "MoveTo",
        "GetDistance",
        "Activate",
        "BlockActivation",
        "IsActivationBlocked",
        "GetLinkedRef",
        "GetItemCount",
        "AddItem",
        "RemoveItem",
        "RemoveAllItems",
        "GetScale",
        "SetScale",
        "GetDisplayName",
        "SetDisplayName",
        "PlayAnimation",
        "GetOpenState",
        "SetOpen",
        "Lock",
        "GetKey",
        "IsLocked",
        "GetLockLevel",
        "SetLockLevel",
        "PlaceAtMe",
        "AddToMap",
        "EnableFastTravel",
        "SetDestroyed",
        "IgnoreFriendlyHits",
        "IsIgnoringFriendlyHits",
        "GetCurrentLocation",
        "GetEditorLocation",
        "IsInLocation",
        "MoveToMyEditorLocation",
        "GetActorOwner",
        "GetFactionOwner",
        "SetActorOwner",
        "SetFactionOwner",
        "AddInventoryEventFilter",
        "RemoveInventoryEventFilter",
        "RemoveAllInventoryEventFilters",
        "SetAnimationVariableFloat",
        "SetAnimationVariableInt",
        "SetAnimationVariableBool",
        "GetAnimationVariableFloat",
        "GetAnimationVariableInt",
        "GetAnimationVariableBool",
        "GetNthLinkedRef",
        "PlaceActorAtMe",
        "Is3DLoaded",
        "IsFurnitureInUse",
        "IsFurnitureMarkerInUse",
        "GetTriggerObjectCount",
        "GetCurrentDestructionStage",
        "GetCurrentScene",
        "IsInDialogueWithPlayer",
        "PlayAnimationAndWait",
        "PlayGamebryoAnimation",
        "SetMotionType",
        "ApplyHavokImpulse",
        "SetNoFavorAllowed",
        "ClearDestruction",
        "DamageObject",
        "KnockAreaEffect",
        "PushActorAway",
        "WaitForAnimationEvent",
        "InterruptCast",
        "ForceAddRagdollToWorld",
        "ForceRemoveRagdollFromWorld",
        "TetherToHorse",
        "CreateDetectionEvent",
        "TranslateTo",
        "SplineTranslateTo",
        "StopTranslation",
        "Reset",
    ];
    for name in methods {
        assert!(
            game.vm.natives.has_method("ObjectReference", name),
            "ObjectReference.{name} from the SCR-67 inventory is missing"
        );
        assert!(
            !game.vm.natives.has_global("ObjectReference", name),
            "ObjectReference.{name} is registered as a global"
        );
    }

    let mut expected: Vec<_> = methods
        .iter()
        .map(|name| ("objectreference".to_string(), name.to_ascii_lowercase()))
        .collect();
    expected.sort_unstable();
    let actual: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "objectreference")
        .collect();
    assert_eq!(actual, expected, "registry-derived ObjectReference methods");
    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "objectreference")
        .collect();
    assert!(
        globals.is_empty(),
        "ObjectReference CK class declares no globals"
    );
}

#[test]
fn scr_67_music_type_bindings_are_methods_and_not_globals() {
    let game = Game::new(LoadOrder::new());
    for name in ["Add", "Remove"] {
        assert!(game.vm.natives.has_method("MusicType", name));
        assert!(!game.vm.natives.has_global("MusicType", name));
    }
    let methods: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "musictype")
        .collect();
    assert_eq!(
        methods,
        [
            ("musictype".into(), "add".into()),
            ("musictype".into(), "remove".into())
        ]
    );
    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "musictype")
        .collect();
    assert!(globals.is_empty(), "MusicType has no registered globals");
}
