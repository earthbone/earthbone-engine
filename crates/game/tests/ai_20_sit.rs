// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-20 Sit procedure reservation, approach and completion.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Location, Schedule};
use soe_game::{Game, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CHAIR: u32 = 0x800;
const CHAIR_BASE: u32 = 0x801;
const SIT_TEMPLATE: u32 = 0x802;
const SIT_PACKAGE: u32 = 0x803;
const ACTOR_BASE: u32 = 0x804;
const ACTOR: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn transform(position: [f32; 3]) -> Vec<u8> {
    let mut data = Vec::new();
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0_u8; 12]);
    data
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let furniture = Record::new(T(b"FURN"), CHAIR_BASE)
        .sub(T(b"MNAM"), 1_u32.to_le_bytes())
        .sub(T(b"FNPR"), [1_u8, 0, 0, 0]);
    plugin.push_top(furniture);
    plugin.push_top(
        Record::new(T(b"REFR"), CHAIR)
            .sub(T(b"NAME"), CHAIR_BASE.to_le_bytes())
            .sub(T(b"DATA"), transform([100.0, 0.0, 0.0])),
    );

    let template =
        package::close_header(package::build_template(SIT_TEMPLATE, "SitProcedure"), None);
    let template = package::with_location_input(template, 0, Location::NearSelf, 0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Sit", &[0], true);
    plugin.push_top(package::with_input_name(template, 0, "Furniture", true));

    let package_record = package::build_record(SIT_PACKAGE, "SitAtChair", 0, Schedule::default());
    let package_record = package::close_header(package_record, Some(SIT_TEMPLATE));
    let package_record =
        package::with_location_input(package_record, 0, Location::NearReference(CHAIR), 0);
    plugin.push_top(package::finish(package_record));

    let actor_base = npc::build_record(
        ACTOR_BASE,
        "SitActor",
        "Sit Actor",
        Config::default(),
        Stats::default(),
    )
    .sub(T(b"PKID"), SIT_PACKAGE.to_le_bytes());
    plugin.push_top(actor_base);
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), transform([0.0, 0.0, 0.0])),
    );

    let mut order = LoadOrder::new();
    order.add("AI20.esm", plugin).unwrap();
    order
}

#[test]
fn ai_20_sit_reserves_the_named_furniture_and_releases_on_arrival() {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));

    game.tick(0.1);
    assert_eq!(
        game.world().furniture_seat(id(ACTOR)),
        Some((id(CHAIR), 0)),
        "the package reserves its furniture Sit marker before approaching: {:?}",
        game.world().actor_ref(id(ACTOR)).unwrap().ai
    );
    let approach_position = game.world().reference_ref(id(ACTOR)).unwrap().position[0];
    assert!(
        approach_position > 0.0 && approach_position < 100.0,
        "the actor walks toward the furniture transform"
    );

    game.tick(2.0);
    assert_eq!(
        game.world().reference_ref(id(ACTOR)).unwrap().position[0],
        100.0,
        "the actor reaches the furniture marker location"
    );
    assert_eq!(game.world().furniture_seat(id(ACTOR)), None);
    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert!(actor.ai.tree.as_ref().unwrap().finished);
    assert!(actor.ai.completed.contains_key(&id(SIT_PACKAGE)));
}

#[test]
fn ai_20_sit_procedure_and_reservation_survive_world_save_load() {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);

    let saved = game.world().save_state();
    let mut restored = World::new(order());
    restored.load_state(&saved).unwrap();

    let actor = restored.actor_ref(id(ACTOR)).unwrap();
    assert_eq!(actor.ai.procedure, soe_game::ai::Procedure::Sit);
    assert_eq!(
        restored.furniture_seat(id(ACTOR)),
        Some((id(CHAIR), 0)),
        "the active procedure retains its marker reservation"
    );
    assert_eq!(restored.save_state(), saved);
}

#[test]
fn ai_20_sit_procedure_is_rejected_by_the_version_37_read_gate() {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);

    let mut saved = game.world().save_state();
    saved[4..6].copy_from_slice(&37_u16.to_le_bytes());
    let mut restored = World::new(order());
    let before = restored.save_state();

    assert!(restored
        .load_state(&saved)
        .unwrap_err()
        .to_string()
        .contains("Sit procedure requires world version 38"));
    assert_eq!(restored.save_state(), before);
}
