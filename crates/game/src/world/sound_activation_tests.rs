// SPDX-License-Identifier: GPL-3.0-or-later

//! Papyrus Sound belongs to SOUN markers, as declared by SKSE PapyrusSound.

use crate::Game;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_papyrus::asm::{b, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, CallMethod, Return};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_77_sound_marker_play_continues_to_linked_activation() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"SOUN"), 20));
    plugin.push_top(Record::new(Tag(*b"ACTI"), 42));
    plugin.push_top(Record::new(Tag(*b"CONT"), 45));
    for (target, base) in [(43_u32, 42_u32), (44, 45)] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), target)
                .sub(b"NAME", base.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
    }
    let mut order = LoadOrder::new();
    order.add("fixture.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("SoundSwitch", "ObjectReference")
        .variable("sound", "Sound", none())
        .variable("linked", "ObjectReference", none())
        .variable("finished", "Bool", b(false))
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("actor", "ObjectReference")
                .local("playing", "Int")
                .local("ok", "Bool")
                .ins(
                    CallMethod,
                    vec![s("play"), ident("sound"), ident("playing"), ident("self")],
                )
                .ins(
                    CallMethod,
                    vec![s("Activate"), ident("linked"), ident("ok"), ident("actor")],
                )
                .ins(Assign, vec![ident("finished"), b(true)])
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let target = game.object_of_form(id(43));
    let sound = game.object_of_form(id(20));
    let child = game.object_of_form(id(44));
    game.vm.attach(target, "SoundSwitch").unwrap();
    game.vm
        .set_var(
            target,
            "SoundSwitch",
            "sound",
            soe_papyrus::Value::Object(sound),
        )
        .unwrap();
    game.vm
        .set_var(
            target,
            "SoundSwitch",
            "linked",
            soe_papyrus::Value::Object(child),
        )
        .unwrap();
    let player = game.player();
    assert!(game.activate(id(43), player));
    assert!(
        game.world_mut().reference(id(44)).unwrap().opened,
        "Sound.Play must not abort the activation before its linked action"
    );
    assert_eq!(game.vm.object_class(sound), Some("Sound"));
    assert_eq!(game.world().sound_instances.len(), 1);
    assert!(game
        .vm
        .get_var(target, "SoundSwitch", "finished")
        .unwrap()
        .as_bool());
    assert!(game
        .vm
        .log
        .entries
        .iter()
        .all(|entry| !entry.message.contains("has no function")));
}

#[test]
fn scr_77_installed_dwemer_sound_marker_has_sound_dispatch() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let order = crate::activation_sweep::installed_order(std::path::Path::new(&data)).unwrap();
    let mut game = Game::from_shared_order(order);
    let sound = game
        .world()
        .form("QSTAstrolabeButtonPressX")
        .expect("installed button sound");
    assert_eq!(game.world().tag_of(sound), Some(Tag(*b"SOUN")));
    let object = game.object_of_form(sound);
    assert_eq!(game.vm.object_class(object), Some("Sound"));
}
