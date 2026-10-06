// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{LoadOrder, Plugin};
use soe_game::Game;
use soe_probe::{parse_schedule_log, Error, ScheduleRow, ScheduleTable};

const EORLUND: u32 = 0x0001_3b93;
const ADRIANNE: u32 = 0x0001_3b94;

fn table() -> ScheduleTable {
    let rows = (0..24)
        .flat_map(|hour| {
            [
                ScheduleRow {
                    hour,
                    actor: EORLUND,
                    name: "Eorlund Gray-Mane".into(),
                    package: Some(0x1000 + u32::from(hour / 8)),
                    cell: Some(0x2000),
                    furniture: (hour == 8).then_some(0x3000),
                },
                ScheduleRow {
                    hour,
                    actor: ADRIANNE,
                    name: "Adrianne Avenicci".into(),
                    package: Some(0x4000),
                    cell: Some(0x2001),
                    furniture: None,
                },
            ]
        })
        .collect();
    ScheduleTable::new(rows).unwrap()
}

#[test]
fn ply_07_schedule_table_requires_every_hour_for_each_unique_named_actor() {
    let table = table();
    assert_eq!(table.rows.len(), 48);
    assert_eq!(table.rows.first().unwrap().hour, 0);
    assert_eq!(table.rows.last().unwrap().hour, 23);
    assert_eq!(
        table.rows.iter().filter(|row| row.actor == EORLUND).count(),
        24
    );
    assert!(table
        .rows
        .iter()
        .filter(|row| row.actor == EORLUND)
        .all(|row| row.name == "Eorlund Gray-Mane"));
}

#[test]
fn ply_07_schedule_table_rejects_duplicate_and_missing_actor_hours() {
    let mut duplicate = table().rows;
    duplicate.push(duplicate[0].clone());
    assert!(matches!(
        ScheduleTable::new(duplicate),
        Err(Error::Invalid(message)) if message.contains("duplicate")
    ));

    let mut missing = table().rows;
    missing.retain(|row| !(row.actor == EORLUND && row.hour == 12));
    assert!(matches!(
        ScheduleTable::new(missing),
        Err(Error::Invalid(message)) if message.contains("every actor-hour")
    ));
}

#[test]
fn ply_07_schedule_comparison_lists_each_changed_actor_hour_field() {
    let engine = table();
    let mut observed = engine.clone();
    let eorlund = observed
        .rows
        .iter_mut()
        .find(|row| row.actor == EORLUND && row.hour == 8)
        .unwrap();
    eorlund.package = Some(0x9000);
    eorlund.cell = Some(0x9001);
    eorlund.furniture = Some(0x9002);
    observed
        .rows
        .retain(|row| !(row.actor == ADRIANNE && row.hour == 3));
    observed.rows.push(ScheduleRow {
        hour: 4,
        actor: 0x5000,
        name: "Extra named NPC".into(),
        package: None,
        cell: None,
        furniture: None,
    });
    // The report accepts imported tables as rows, including their missing and
    // extra samples, so comparisons can name incomplete original captures.
    let observed = ScheduleTable {
        rows: observed.rows,
    };
    let differences = engine.differences(&observed);
    assert_eq!(
        differences
            .iter()
            .filter(|difference| difference.actor == EORLUND && difference.hour == 8)
            .map(|difference| difference.field.as_str())
            .collect::<Vec<_>>(),
        ["package", "cell", "furniture"]
    );
    assert!(differences.iter().any(|difference| {
        difference.actor == ADRIANNE && difference.hour == 3 && difference.field == "row"
    }));
    assert!(differences.iter().any(|difference| {
        difference.actor == 0x5000 && difference.hour == 4 && difference.field == "row"
    }));
}

#[test]
fn ply_07_schedule_log_parser_preserves_missing_rows_for_comparison() {
    let complete = table();
    let lines = complete
        .rows
        .iter()
        .map(|row| format!("SOESCHEDULE {}", serde_json::to_string(&row).unwrap()))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse_schedule_log(&lines).unwrap(), complete);

    let incomplete = complete
        .rows
        .iter()
        .filter(|row| !(row.actor == EORLUND && row.hour == 12))
        .map(|row| format!("SOESCHEDULE {}", serde_json::to_string(row).unwrap()))
        .collect::<Vec<_>>()
        .join("\n");
    let observed = parse_schedule_log(&incomplete).unwrap();
    assert!(complete.differences(&observed).iter().any(|difference| {
        difference.actor == EORLUND && difference.hour == 12 && difference.field == "row"
    }));
}

#[test]
fn ply_07_schedule_installed_eorlund_is_a_named_whiterun_actor() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed Eorlund schedule source check");
        return;
    };
    let bytes = std::fs::read(format!("{data}/Skyrim.esm")).unwrap();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let game = Game::new(order);
    let actor = game.world().form("EorlundGrayMane").unwrap();
    assert_eq!(
        game.world().editor_id(actor).as_deref(),
        Some("EorlundGrayMane")
    );
    let placed = game.world().references_of_base(actor, 8);
    assert!(!placed.is_empty(), "Eorlund has a placed actor reference");
    let eorlund_ref = placed
        .into_iter()
        .find(|reference| {
            game.world()
                .tag_of(*reference)
                .is_some_and(|tag| tag == soe_esm::Tag(*b"ACHR"))
        })
        .expect("Eorlund is placed as an actor");
    game.world_mut().drive_actor(eorlund_ref);
    let mut world = game.world_mut();
    assert!(world.reference(eorlund_ref).is_some());
    assert!(world.reference(eorlund_ref).unwrap().cell.is_some());
}
