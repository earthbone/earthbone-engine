// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{UnresolvedRecord, UnresolvedRecordId};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn live_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x500).sub(Tag(*b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x101)
            .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Live.esm", plugin).unwrap();
    order
}

fn source_order() -> LoadOrder {
    let mut order = live_order();
    order.add("Gone.esp", Plugin::new(FileHeader::default())).unwrap();
    order
        .add(
            "GoneLight.esl",
            Plugin::new(FileHeader {
                flags: soe_esm::plugin::flags::LIGHT_MASTER,
                ..FileHeader::default()
            }),
        )
        .unwrap();
    order
}

#[test]
fn sav_58_removed_records_keep_exact_links_and_unrelated_pose_for_two_native_cycles() {
    let regular = UnresolvedRecord {
        id: UnresolvedRecordId {
            plugin: "gone.esp".into(),
            light: false,
            object: 0x800,
        },
        payload: vec![0, 0xff, 0x81, 0x00, 0x42],
        links: vec![UnresolvedRecordId {
            plugin: "gonelight.esl".into(),
            light: true,
            object: 0x800,
        }],
    };
    let light = UnresolvedRecord {
        id: regular.links[0].clone(),
        payload: vec![0x10, 0x00, 0x99, 0xfe],
        links: vec![regular.id.clone()],
    };
    let source = Game::new(source_order());
    {
        let mut world = source.world_mut();
        world.retain_unresolved_record(regular.clone()).unwrap();
        world.retain_unresolved_record(light.clone()).unwrap();
        assert!(world.set_host_actor_pose(id(0x101), [15.0, -30.0, 7.0], [0.1, 0.2, 0.3]));
    }
    let first = source.save_state().unwrap();

    let mut after_removal = Game::new(live_order());
    after_removal.load_state(&first).unwrap();
    let second = after_removal.save_state().unwrap();
    let mut reloaded = Game::new(live_order());
    reloaded.load_state(&second).unwrap();

    assert_eq!(reloaded.world().unresolved_records().get(&regular.id), Some(&regular));
    assert_eq!(reloaded.world().unresolved_records().get(&light.id), Some(&light));
    let placement = reloaded.world().ref_placement(id(0x101)).unwrap();
    assert_eq!(placement.position, [15.0, -30.0, 7.0]);
    assert_eq!(placement.rotation, [0.1, 0.2, 0.3]);
    assert_eq!(reloaded.world().record(id(0x800)).unwrap().tag, Tag(*b"STAT"));
}
