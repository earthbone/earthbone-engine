// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, World, natives};
use soe_papyrus::Vm;

#[test]
fn actorbase_alias_audit_covers_registered_bindings_and_assigns_gaps() {
    let mut vm = Vm::new();
    let world = Rc::new(RefCell::new(World::new(LoadOrder::new())));
    natives::register(&mut vm, &world);

    let actor_base = [
        "GetSkin",
        "SetSkin",
        "GetRace",
        "GetClass",
        "GetSex",
        "GetDeadCount",
        "IsEssential",
        "IsProtected",
        "IsInvulnerable",
        "IsUnique",
        "SetEssential",
        "SetProtected",
        "SetInvulnerable",
        "SetOutfit",
        "GetGiftFilter",
    ];
    let alias = [
        "GetOwningQuest",
        "GetID",
        "GetName",
        "GetReference",
        "GetRef",
        "GetActorReference",
        "GetActorRef",
        "ForceRefTo",
        "Clear",
    ];
    let inherited_form = [
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
    ];
    let audit = include_str!("../../../docs/reference-audit.md");

    for name in actor_base {
        assert!(vm.natives.has_method("ActorBase", name));
        assert!(audit.contains(&format!("`ActorBase.{name}`")));
    }
    for name in alias {
        assert!(vm.natives.has_method("Alias", name));
        assert!(audit.contains(&format!("`Alias.{name}`")));
    }
    for name in inherited_form {
        assert!(vm.natives.has_method("Form", name));
        assert!(audit.contains(&format!("`Form.{name}`")));
    }
    assert!(audit.contains("ActorBase_Script.wikitext"));
    assert!(audit.contains("Alias_Script.wikitext"));
    assert!(audit.contains("ReferenceAlias_Script.wikitext"));
    assert!(audit.contains("SCR-18"));
    assert!(audit.contains("SCR-59"));
    assert!(audit.contains("SCR-68 owns both gaps"));
    let backlog = include_str!("../../../BACKLOG.csv");
    for row in ["SCR-18", "SCR-59", "SCR-68"] {
        assert!(
            backlog.contains(&format!("\"{row}\"")),
            "missing owner row {row}"
        );
    }
}

#[test]
fn setting_actor_base_flags_keeps_essential_and_protected_mutually_exclusive() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 1));
    let mut order = LoadOrder::new();
    order
        .add("scr55-breaker.esp", plugin)
        .expect("plugin loads");
    let game = Game::new(order);
    let base = GlobalFormId { file: 0, object: 1 };
    game.world_mut().set_actor_base_essential(base, true);
    game.world_mut().set_actor_base_protected(base, true);
    let mut world = game.world_mut();
    let state = world.actor_base(base).expect("NPC_ state");
    assert!(
        !(state.essential && state.protected),
        "CK forbids both flags on one ActorBase"
    );
}
