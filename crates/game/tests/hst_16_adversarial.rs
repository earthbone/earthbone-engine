// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::Pending, Game};
use soe_papyrus::{
    asm::{f, ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::CallMethod,
};

const REF: u32 = 0x900;

fn id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: REF,
    }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REF)
            .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Translate.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_16_translate_native_converts_angles_and_honors_rotation_speed() {
    let mut game = game();
    let script = ScriptBuilder::new("Hst16Probe", "ObjectReference")
        .function(
            FunctionBuilder::new("Run", "None")
                .local("result", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("TranslateTo"),
                        ident("self"),
                        ident("result"),
                        f(10.0),
                        f(0.0),
                        f(0.0),
                        f(0.0),
                        f(0.0),
                        f(90.0),
                        f(10.0),
                        f(30.0),
                    ],
                )
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.object_of_form(id());
    game.vm.attach(object, "Hst16Probe").unwrap();
    game.vm.call(object, "Run", &[]).unwrap();
    assert!(game.run_until_idle(32));
    game.world_mut()
        .tick_translations(0.5, &mut |_, _| soe_papyrus::ObjectId(1));

    let world = game.world();
    let reference = world.reference_ref(id()).unwrap();
    assert_eq!(reference.position, [5.0, 0.0, 0.0]);
    assert!((reference.rotation[2].to_degrees() - 15.0).abs() < 0.001);
    assert!(!game.world().pending.iter().any(|event| matches!(event,
        Pending::Event { name, .. } if name == "OnTranslationComplete"
    )));
}
