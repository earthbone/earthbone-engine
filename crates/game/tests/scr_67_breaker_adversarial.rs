// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const MUSIC_A: u32 = 0x801;
const MUSIC_B: u32 = 0x802;
const NOT_MUSIC: u32 = 0x803;
const ITEM: u32 = 0x804;
const OWNER: u32 = 0x805;
const SOURCE_BASE: u32 = 0x806;
const TARGET_BASE: u32 = 0x807;
const SOURCE: u32 = 0x808;
const TARGET: u32 = 0x809;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MUSC"), MUSIC_A));
    plugin.push_top(Record::new(T(b"MUSC"), MUSIC_B));
    plugin.push_top(Record::new(T(b"STAT"), NOT_MUSIC));
    plugin.push_top(Record::new(T(b"MISC"), ITEM));
    plugin.push_top(Record::new(T(b"NPC_"), OWNER));
    plugin.push_top(Record::new(T(b"CONT"), SOURCE_BASE));
    plugin.push_top(Record::new(T(b"CONT"), TARGET_BASE));
    plugin.push_top(
        Record::new(T(b"REFR"), SOURCE)
            .zsub(T(b"EDID"), "Source")
            .sub(T(b"NAME"), SOURCE_BASE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), TARGET)
            .zsub(T(b"EDID"), "Target")
            .sub(T(b"NAME"), TARGET_BASE.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, form: GlobalFormId, method: &str) -> Value {
    let name = format!("Scr67Breaker{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![s(method), ident("target"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let script = ScriptBuilder::new(&name, "Form").function(function).bytes();
    game.load_script(&script).unwrap();
    let target = Value::Object(game.object_of_form(form));
    let thread = game.vm.call_global(&name, "Run", &[target]).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn remove_all(game: &mut Game, source: GlobalFormId, target: GlobalFormId, keep_owner: bool) {
    let name = format!("Scr67RemoveAll{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("source", "Form")
        .param("target", "Form")
        .param("keepOwner", "Bool")
        .param("removeQuestItems", "Bool")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![
                s("RemoveAllItems"),
                ident("source"),
                ident("result"),
                ident("target"),
                ident("keepOwner"),
                ident("removeQuestItems"),
            ],
        )
        .ins(Return, vec![])
        .build();
    let script = ScriptBuilder::new(&name, "Form").function(function).bytes();
    game.load_script(&script).unwrap();
    let source = Value::Object(game.object_of_form(source));
    let target = Value::Object(game.object_of_form(target));
    let thread = game
        .vm
        .call_global(
            &name,
            "Run",
            &[source, target, Value::Bool(keep_owner), Value::Bool(false)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(
        game.vm.thread_result(thread).unwrap_or(Value::None),
        Value::None
    );
}

#[test]
fn scr_67_breaker_registry_sets_have_no_unlisted_class_bindings() {
    let game = Game::new(LoadOrder::new());
    let expected_reference = [
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
    let mut expected: Vec<_> = expected_reference
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
    assert_eq!(actual, expected);
    assert!(game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .all(|(class, _)| class != "objectreference"));
}

#[test]
fn scr_67_breaker_music_type_calls_are_idempotent_and_receiver_scoped() {
    let mut game = game();
    for method in ["Add", "Add"] {
        assert_eq!(call(&mut game, id(MUSIC_A), method), Value::None);
    }
    assert_eq!(
        game.world().active_music_types(),
        [id(MUSIC_A)],
        "repeating Add must not duplicate an active music type"
    );
    assert_eq!(call(&mut game, id(NOT_MUSIC), "Add"), Value::None);
    assert_eq!(game.world().active_music_types(), [id(MUSIC_A)]);
}

#[test]
fn scr_67_breaker_music_remove_leaves_other_active_types_unchanged() {
    let mut game = game();
    call(&mut game, id(MUSIC_A), "Add");
    call(&mut game, id(MUSIC_B), "Add");
    assert_eq!(call(&mut game, id(MUSIC_A), "Remove"), Value::None);
    assert_eq!(call(&mut game, id(MUSIC_A), "Remove"), Value::None);
    assert_eq!(game.world().active_music_types(), [id(MUSIC_B)]);
}

#[test]
fn scr_67_breaker_remove_all_items_drops_owner_when_keep_ownership_is_false() {
    let mut game = game();
    let source = game.form("Source").unwrap();
    let target = game.form("Target").unwrap();
    let owner = id(OWNER);
    game.world_mut()
        .reference(source)
        .unwrap()
        .inventory
        .restore(
            id(ITEM),
            1,
            vec![Stack::new(
                1,
                vec![Extra {
                    kind: 0x21,
                    fields: vec![Field::Form(Some(owner))],
                }],
            )
            .unwrap()],
        )
        .unwrap();

    remove_all(&mut game, source, target, false);

    let mut world = game.world_mut();
    let recipient = world.reference(target).unwrap();
    assert_eq!(recipient.inventory.count(id(ITEM)), 1);
    assert!(recipient.inventory.stacks(id(ITEM))[0]
        .extra(0x21)
        .is_none());
}
