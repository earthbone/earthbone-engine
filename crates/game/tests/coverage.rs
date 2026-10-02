// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Tag};
use soe_game::{
    Game, Handle, harness, quest,
    vmad::{AliasScripts, ObjectRef, ScriptEntry, Vmad},
};
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::coverage::Summary;

#[test]
fn quest_coverage_uses_own_objects_aliases_and_unique_inherited_bodies() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (id, name) in [(0x800, "First"), (0x801, "Second")] {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("Shared")],
            aliases: vec![AliasScripts {
                object: ObjectRef {
                    form_id: id,
                    alias: 0,
                },
                scripts: vec![ScriptEntry::new("Shared")],
            }],
            ..Vmad::default()
        };
        let record = quest::with_alias(quest::build_record(id, name, 0), 0, "Alias", None)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST")));
        plugin.push_top(record);
    }
    let missing = Vmad {
        scripts: vec![ScriptEntry::new("Missing")],
        ..Vmad::default()
    };
    plugin.push_top(
        quest::build_record(0x802, "MissingQuest", 0)
            .sub(Tag::new(b"VMAD"), missing.write(Tag::new(b"QUST"))),
    );
    plugin
        .push_top(quest::build_record(0x803, "InvalidQuest", 0).sub(Tag::new(b"VMAD"), vec![0u8]));
    let mut deleted_root = ScriptEntry::new("MissingDeleted");
    deleted_root.status = 3;
    let mut deleted_alias = ScriptEntry::new("Shared");
    deleted_alias.status = 3;
    let deleted = Vmad {
        scripts: vec![deleted_root],
        aliases: vec![AliasScripts {
            object: ObjectRef {
                form_id: 0x804,
                alias: 0,
            },
            scripts: vec![deleted_alias],
        }],
        ..Vmad::default()
    };
    plugin.push_top(
        quest::with_alias(
            quest::build_record(0x804, "DeletedQuest", 0),
            0,
            "Alias",
            None,
        )
        .sub(Tag::new(b"VMAD"), deleted.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(
        &ScriptBuilder::new("Parent", "Quest")
            .function(FunctionBuilder::new("Inherited", "None").build())
            .bytes(),
    )
    .unwrap();
    game.load_script(
        &ScriptBuilder::new("Shared", "Parent")
            .function(FunctionBuilder::new("Run", "None").build())
            .function(FunctionBuilder::new("Uncalled", "None").build())
            .bytes(),
    )
    .unwrap();
    game.bind();
    game.vm.start_coverage();
    let first = game.form("First").unwrap();
    let alias = game.object_of(Handle::Alias {
        quest: first,
        id: 0,
    });
    game.vm.call(alias, "Inherited", &[]).unwrap();
    game.vm.run_until_idle(100);
    let scenario = harness::Scenario::parse("event First Run").unwrap();
    let report = harness::run(&mut game, &scenario);
    assert!(report.passed());
    let coverage = report.coverage.unwrap();
    assert_eq!(
        coverage.quests[0].summary,
        Summary {
            executed: 2,
            total: 3
        }
    );
    assert_eq!(
        coverage.quests[1].summary,
        Summary {
            executed: 0,
            total: 3
        }
    );
    assert!(coverage.quests[2].missing_scripts.contains("missing"));
    assert_eq!(coverage.quests[2].summary.percent(), None);
    assert!(coverage.quests[3].vmad_error.is_some());
    assert!(coverage.quests[4].missing_scripts.is_empty());
    assert_eq!(coverage.quests[4].summary, Summary::default());
    assert_eq!(
        coverage.to_text(),
        game.coverage_report().unwrap().to_text()
    );
    assert!(coverage.to_text().contains("script shared: 1/2 (50.00%)"));
}
