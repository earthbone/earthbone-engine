// SPDX-License-Identifier: GPL-3.0-or-later

//! Dwemer switch coverage from installed ACTI models and their authored animation.

use super::*;

#[test]
fn activation_sweep_dwemer_switch_models_have_executable_surfaces() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (_, record) in source.order.winners_of(Tag(*b"ACTI")) {
        let name = record.editor_id().unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        if !lower.contains("dwe") || !(lower.contains("button") || lower.contains("lever")) {
            continue;
        }
        let Some(raw) = record.find(Tag(*b"MODL")) else {
            continue;
        };
        let path = String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string();
        let surfaces = source.animated_model_surfaces(&path).unwrap();
        checked += 1;
        if !surfaces.iter().any(|surface| surface.animation.is_some()) {
            missing.push((name, path));
        }
    }
    assert!(
        checked >= 2,
        "installed Dwemer button and lever bases must be covered"
    );
    assert!(
        missing.is_empty(),
        "frozen Dwemer switch models: {missing:?}"
    );
}

#[test]
fn activation_sweep_dwemer_events_move_bound_nodes_and_complete_waits() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    for (name, initial, event, complete) in [
        ("DweButton01", "open", "Trigger01", "done"),
        ("DweLever01", "FullPull", "FullPush", "FullPushedUp"),
    ] {
        let base = source
            .order
            .winners_of(Tag(*b"ACTI"))
            .find(|(_, record)| record.editor_id().as_deref() == Some(name))
            .unwrap()
            .0;
        let record = source.order.winner(base).unwrap();
        let path = String::from_utf8_lossy(record.find(Tag(*b"MODL")).unwrap())
            .trim_end_matches('\0')
            .to_string();
        let file = placed_model(&source.vfs.read(&format!("meshes/{path}")).unwrap()).unwrap();
        let animation = soe_nif::object_animation::ObjectAnimation::from_file(&file).unwrap();
        let rest = animation.rest_pose();
        let asset = crate::object_behavior::Asset::from_nif(&source.vfs, &file, rest.clone())
            .unwrap()
            .expect("switch behavior project");
        println!("{name} authored events: {:?}", asset.events());
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        plugin.push_top(soe_esm::Record::new(Tag(*b"ACTI"), 42));
        plugin.push_top(
            soe_esm::Record::new(Tag(*b"REFR"), 43)
                .sub(b"NAME", 42_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("fixture.esm", plugin).unwrap();
        let mut game = soe_game::Game::new(order);
        let target = GlobalFormId {
            file: 0,
            object: 43,
        };
        game.world_mut().reference(target).unwrap();
        game.report_animation_loaded(target);
        let mut runtime = crate::object_behavior::Runtime::new(asset, target);
        game.world_mut().request_animation(target, initial);
        for _ in 0..180 {
            runtime.update(&mut game, 1.0 / 60.0).unwrap();
        }
        let before = runtime.pose.clone();
        use soe_papyrus::asm::{b, ident, none, s, FunctionBuilder, ScriptBuilder};
        use soe_papyrus::Opcode::{Assign, CallMethod, Return};
        let script = ScriptBuilder::new("SwitchWaitCheck", "ObjectReference")
            .variable("finished", "Bool", b(false))
            .function(
                FunctionBuilder::new("OnActivate", "None")
                    .param("actor", "ObjectReference")
                    .local("ok", "Bool")
                    .ins(
                        CallMethod,
                        vec![
                            s("PlayAnimationAndWait"),
                            ident("self"),
                            ident("ok"),
                            s(event),
                            s(complete),
                        ],
                    )
                    .ins(Assign, vec![ident("finished"), ident("ok")])
                    .ins(Return, vec![none()])
                    .build(),
            )
            .bytes();
        game.load_script(&script).unwrap();
        let object = game.object_of_form(target);
        game.vm.attach(object, "SwitchWaitCheck").unwrap();
        let player = game.player();
        assert!(game.activate(target, player));
        let mut moved = false;
        for _ in 0..180 {
            runtime.update(&mut game, 1.0 / 60.0).unwrap();
            moved |= before
                .transforms
                .iter()
                .zip(&runtime.pose.transforms)
                .any(|(a, b)| {
                    a.translation
                        .to_array()
                        .iter()
                        .zip(b.translation.to_array())
                        .any(|(a, b)| (a - b).abs() > 0.001)
                        || a.rotation
                            .rows
                            .iter()
                            .flatten()
                            .zip(b.rotation.rows.iter().flatten())
                            .any(|(a, b)| (a - b).abs() > 0.001)
                });
        }
        assert!(moved,"{name} {event} must move an authored mechanism by at least 0.001 units or matrix components");
        assert!(
            game.vm
                .get_var(object, "SwitchWaitCheck", "finished")
                .unwrap()
                .as_bool(),
            "{name} must emit {complete} and finish its script wait"
        );
    }
}
