// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

const PLAYER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x14,
};
const MGEF: u32 = 0x100;
const POTION: u32 = 0x101;
const QUEST: u32 = 0x200;
const DESTINATION: u32 = 0x300;
const ALIAS: u32 = 1;

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 64, 25);
    put(&mut effect_data, 80, 1);
    put(&mut effect_data, 84, 3);
    plugin.push_top(Record::new(tag(*b"MGEF"), MGEF).sub(tag(*b"DATA"), effect_data));

    let mut quest = Record::new(tag(*b"QUST"), QUEST).sub(tag(*b"DNAM"), {
        let mut data = vec![0; 12];
        data[2] = 50;
        data
    });
    quest = quest
        .sub(tag(*b"QOBJ"), 1u16.to_le_bytes())
        .sub(
            tag(*b"QSTA"),
            [ALIAS.to_le_bytes(), 0u32.to_le_bytes()].concat(),
        )
        .sub(tag(*b"ALST"), ALIAS.to_le_bytes())
        .zsub(tag(*b"ALID"), "GuideTarget")
        .sub(tag(*b"ALFR"), DESTINATION.to_le_bytes());
    plugin.push_top(quest);
    plugin.push_top(
        Record::new(tag(*b"REFR"), DESTINATION)
            .sub(tag(*b"NAME"), 0x400u32.to_le_bytes())
            .sub(tag(*b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(tag(*b"MISC"), 0x400));
    let item_data = vec![0; 20];
    plugin.push_top(
        Record::new(tag(*b"ALCH"), POTION)
            .sub(tag(*b"ENIT"), item_data)
            .sub(tag(*b"EFID"), MGEF.to_le_bytes())
            .sub(
                tag(*b"EFIT"),
                [1.0f32.to_le_bytes(), 0u32.to_le_bytes(), 5u32.to_le_bytes()].concat(),
            ),
    );

    let mut order = LoadOrder::new();
    order.add("Guide.esm", plugin).unwrap();
    let game = Game::new(order);
    let player = game.world_mut().player();
    game.world_mut().reference(player).unwrap().position = [0.0, 0.0, 0.0];
    game.world_mut()
        .reference(GlobalFormId {
            file: 0,
            object: DESTINATION,
        })
        .unwrap()
        .position = [128.0, 64.0, 0.0];
    {
        let mut world = game.world_mut();
        let quest = world
            .quest_state(GlobalFormId {
                file: 0,
                object: QUEST,
            })
            .unwrap();
        quest.running = true;
        quest.active = true;
        quest.objectives.entry(1).or_default().displayed = true;
    }
    game.world_mut().add_item(
        PLAYER,
        GlobalFormId {
            file: 0,
            object: POTION,
        },
        1,
    );
    game
}

fn apply_guide(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .drink_ingestible(
            GlobalFormId {
                file: 0,
                object: POTION,
            },
            PLAYER,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_46_guide_routes_to_the_current_eligible_objective() {
    let mut game = fixture();
    apply_guide(&mut game);

    let route = game.world_mut().guide_route().unwrap();
    assert_eq!(
        route.quest,
        GlobalFormId {
            file: 0,
            object: QUEST
        }
    );
    assert_eq!(route.objective, 1);
    assert_eq!(
        route.destination,
        GlobalFormId {
            file: 0,
            object: DESTINATION
        }
    );
    assert_eq!(route.points.last(), Some(&[128.0, 64.0, 0.0]));
    assert!(route.points.iter().flatten().all(|point| point.is_finite()));
}

#[test]
fn mag_46_guide_clears_the_route_when_its_effect_expires() {
    let mut game = fixture();
    apply_guide(&mut game);
    assert!(game.world_mut().guide_route().is_some());

    game.tick(5.0);

    assert!(game.world_mut().guide_route().is_none());
}

#[test]
fn mag_46_guide_skips_completed_and_failed_objectives() {
    let mut game = fixture();
    apply_guide(&mut game);
    game.world_mut()
        .objective(
            GlobalFormId {
                file: 0,
                object: QUEST,
            },
            1,
        )
        .unwrap()
        .completed = true;

    assert!(game.world_mut().guide_route().is_none());

    let mut world = game.world_mut();
    let state = world
        .objective(
            GlobalFormId {
                file: 0,
                object: QUEST,
            },
            1,
        )
        .unwrap();
    state.completed = false;
    state.failed = true;
    drop(world);
    assert!(game.world_mut().guide_route().is_none());
}
