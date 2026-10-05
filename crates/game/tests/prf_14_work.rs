// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Fixed quest bursts and installed navigation queries for PRF-14.

use soe_esm::records::gameplay::{Comparison, Condition};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::vmad::{QuestFragment, ScriptEntry, Vmad};
use soe_game::{conditions, quest, Game};
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode;
use soe_trace::{Operation, WorkCounts, WorkFrame};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn burst() -> WorkCounts {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 0x800..0x820 {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("Burst")],
            fragment_file: Some("Burst".into()),
            fragments: vec![QuestFragment {
                stage: 10,
                log_entry: 0,
                script: "Burst".into(),
                function: "Fragment_10".into(),
            }],
            ..Default::default()
        };
        let record = quest::build_record(object, &format!("Burst{object}"), 0)
            .sub(Tag(*b"VMAD"), vmad.write(Tag(*b"QUST")));
        plugin.push_top(quest::with_stage(record, 10, 0, 0, "Burst stage"));
    }
    let mut order = LoadOrder::new();
    order.add("Burst.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let script = ScriptBuilder::new("Burst", "Quest")
        .variable("completed", "Int", i(0))
        .function(
            FunctionBuilder::new("Fragment_10", "None")
                .local("stage", "Int")
                .ins(
                    Opcode::CallMethod,
                    vec![s("GetStage"), ident("self"), ident("stage")],
                )
                .ins(
                    Opcode::IAdd,
                    vec![ident("completed"), ident("completed"), i(1)],
                )
                .ins(Opcode::Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game.bind();
    game.run_until_idle(64);

    let limits = WorkCounts([0, 0, 0, 96, 32, 32, 0, 0, 8_192, 0]);
    let frame = WorkFrame::begin();
    for object in 0x800..0x820 {
        assert!(game.set_stage(id(object), 10));
        let context = conditions::Context {
            owner: id(object),
            subject: id(object),
            quest: Some(id(object)),
            target: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        };
        let first = Condition {
            flags: 1,
            comparison: Comparison::Number(10.0),
            function: 58,
            parameters: [object, 0],
            run_on: 0,
            reference: 0,
            parameter_3: -1,
            strings: [None, None],
        };
        let skipped = Condition {
            flags: 0,
            function: u16::MAX,
            ..first.clone()
        };
        assert_eq!(
            conditions::evaluate(&mut game.world_mut(), &game.vm, context, &[first, skipped]),
            Ok(true)
        );
    }
    game.tick(1.0 / 60.0);
    let counts = frame.counts();
    assert_eq!(counts.check(&limits), Ok(()), "quest burst {counts:?}");
    assert_eq!(counts.get(Operation::PapyrusInstructions), 96);
    assert_eq!(counts.get(Operation::NativeCalls), 32);
    assert_eq!(counts.get(Operation::Conditions), 32);
    assert!(counts.get(Operation::RecordLookups) > 0);
    assert!(game.vm.log.entries.is_empty(), "{:?}", game.vm.log.entries);
    drop(frame);
    for object in 0x800..0x820 {
        let object = game.object_of_form(id(object));
        assert_eq!(
            game.vm
                .get_var(object, "Burst", "completed")
                .unwrap()
                .as_int(),
            1
        );
    }
    game.tick(1.0 / 60.0);
    assert_eq!(
        game.last_work_counts().get(Operation::PapyrusInstructions),
        0
    );
    assert_eq!(game.last_work_counts().get(Operation::NativeCalls), 0);
    counts
}

#[test]
fn prf_14_quest_burst_bounds_fragments_natives_conditions_and_record_work() {
    assert_eq!(burst(), burst(), "fixed bursts have identical counts");
}

#[test]
fn prf_14_failed_conditions_are_counted_and_work_does_not_enter_saves() {
    let game = Game::new(LoadOrder::new());
    let before = game.world().save_state();
    let frame = WorkFrame::begin();
    assert!(game.world().record(id(0x999)).is_none());
    let c = Condition {
        flags: 0,
        comparison: Comparison::Number(0.0),
        function: u16::MAX,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    let context = conditions::Context {
        owner: id(0),
        subject: id(0),
        quest: None,
        target: None,
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    };
    assert!(conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, &c).is_err());
    assert_eq!(frame.counts().get(Operation::Conditions), 1);
    assert!(frame.counts().get(Operation::RecordLookups) >= 1);
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn prf_14_vanilla_bannered_mare_navmesh_queries_have_bounded_triangle_work() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let cell = order
        .winners_of(Tag(*b"CELL"))
        .find(|(_, record)| record.editor_id().as_deref() == Some("WhiterunBanneredMare"))
        .unwrap()
        .0;
    let (graph, stats) = soe_navmesh::vanilla::from_load_order(&order, |record| {
        record.placement.cell == Some(cell.object)
    });
    assert!(stats.triangles > 0, "vanilla NAVM records were selected");
    let limits = WorkCounts([1, 256, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut sampled = 0;
    for (mi, mesh) in graph.meshes().iter().enumerate() {
        for ti in (0..mesh.triangles.len()).step_by(7) {
            let point = graph.centroid(soe_navmesh::NodeId::new(mi as u32, ti as u32));
            let frame = WorkFrame::begin();
            assert!(graph.locate(point).is_some());
            let counts = frame.counts();
            assert_eq!(counts.check(&limits), Ok(()), "vanilla query {counts:?}");
            assert!(counts.get(Operation::NavmeshTriangles) > 0);
            sampled += 1;
        }
    }
    assert!(sampled >= 10, "sampled {sampled} vanilla triangles");
}
