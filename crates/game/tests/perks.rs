// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PERK purchase transactions, NNAM rank identities and installed-data survey.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    conditions,
    perks::{Error, Purchase},
    Game,
};
use std::collections::{BTreeMap, BTreeSet};

const FIRST: u32 = 0x800;
const SECOND: u32 = 0x801;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn condition(function: u16, parameter: u32, value: f32, flags: u8) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[0] = flags;
    bytes[4..8].copy_from_slice(&value.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn perk(raw: u32) -> Record {
    Record::new(Tag(*b"PERK"), raw).sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Perks.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        let player = world.player();
        world.actor(player).unwrap();
        world.perk_points = 5;
    }
    game
}

fn state(game: &Game) -> (i32, BTreeSet<GlobalFormId>) {
    let mut world = game.world_mut();
    let player = world.player();
    (
        world.perk_points,
        world.actor(player).unwrap().perks.clone(),
    )
}

fn chain() -> Vec<Record> {
    vec![
        perk(FIRST).sub(Tag(*b"NNAM"), SECOND.to_le_bytes()),
        perk(SECOND)
            .sub(Tag(*b"CTDA"), condition(448, FIRST, 1.0, 0))
            .sub(Tag(*b"CTDA"), condition(277, 6, 40.0, 0x60)),
    ]
}

#[test]
fn prg_02_purchase_spends_one_point_and_failed_prerequisite_preserves_ranks() {
    let mut game = game(chain());
    let purchase = game.purchase_perk(id(FIRST)).unwrap();
    assert_eq!((purchase.perk, purchase.rank), (id(FIRST), 1));
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
    assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Ineligible));
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
}

#[test]
fn purchase_rechecks_preview_and_saves_each_rank_identity() {
    let mut records = chain();
    records[1].subrecords.pop();
    records[1] = records[1]
        .clone()
        .sub(Tag(*b"CTDA"), condition(277, 6, 40.0, 0x60));
    let mut game = game(records);
    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "onehanded", 50.0)
        .unwrap();
    game.purchase_perk(id(FIRST)).unwrap();
    let preview = game.check_perk_purchase(id(FIRST)).unwrap();
    assert_eq!((preview.perk, preview.rank), (id(SECOND), 2));
    game.world_mut()
        .set_actor_value(player, "onehanded", 39.0)
        .unwrap();
    game.world_mut()
        .mod_temporary_actor_value(player, "onehanded", 50.0)
        .unwrap();
    assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Ineligible));
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
    game.world_mut()
        .set_actor_value(player, "onehanded", 40.0)
        .unwrap();
    assert_eq!(game.purchase_perk(id(FIRST)).unwrap(), preview);
    let expected = state(&game);
    let hash = game.world().state_hash();
    let bytes = game.save_state().unwrap();
    game.world_mut().perk_points = 0;
    game.world_mut().actor(player).unwrap().perks.clear();
    game.load_state(&bytes).unwrap();
    assert_eq!(state(&game), expected);
    assert_eq!(game.world().state_hash(), hash);
    assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Complete));
}

#[test]
fn invalid_roots_cycles_and_missing_links_do_not_spend_points() {
    for (records, requested, error) in [
        (chain(), SECOND, Error::NotRoot(id(SECOND))),
        (
            vec![perk(FIRST).sub(Tag(*b"NNAM"), FIRST.to_le_bytes())],
            FIRST,
            Error::Cycle(id(FIRST)),
        ),
        (
            vec![perk(FIRST).sub(Tag(*b"NNAM"), SECOND.to_le_bytes())],
            FIRST,
            Error::Perk(id(SECOND)),
        ),
        (
            vec![perk(FIRST).sub(Tag(*b"NNAM"), [1, 2])],
            FIRST,
            Error::Perk(id(FIRST)),
        ),
        (
            vec![Record::new(Tag(*b"PERK"), FIRST)],
            FIRST,
            Error::Perk(id(FIRST)),
        ),
        (
            vec![Record::new(Tag(*b"MISC"), FIRST)],
            FIRST,
            Error::Perk(id(FIRST)),
        ),
    ] {
        let mut game = game(records);
        let before = state(&game);
        assert_eq!(game.purchase_perk(id(requested)), Err(error));
        assert_eq!(state(&game), before);
    }
}

#[test]
fn deleted_flagged_and_malformed_records_preserve_purchase_state() {
    let mut deleted = perk(FIRST);
    deleted.flags |= 0x20;
    let mut flagged = perk(FIRST);
    flagged.flags |= 4;
    for (record, error) in [
        (deleted, Error::Perk(id(FIRST))),
        (flagged, Error::NotPlayable(id(FIRST))),
        (
            perk(FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 1, 0]),
            Error::Perk(id(FIRST)),
        ),
        (
            perk(FIRST)
                .sub(Tag(*b"NNAM"), 0_u32.to_le_bytes())
                .sub(Tag(*b"NNAM"), 0_u32.to_le_bytes()),
            Error::Perk(id(FIRST)),
        ),
    ] {
        let mut game = game(vec![record]);
        assert_eq!(game.purchase_perk(id(FIRST)), Err(error));
        assert_eq!(state(&game), (5, BTreeSet::new()));
    }
    let mut game = game(vec![perk(FIRST).sub(Tag(*b"CTDA"), [0; 4])]);
    assert!(matches!(
        game.purchase_perk(id(FIRST)),
        Err(Error::Format(_))
    ));
    assert_eq!(state(&game), (5, BTreeSet::new()));
}

#[test]
fn effect_conditions_do_not_become_purchase_prerequisites() {
    let record = perk(FIRST)
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [0, 1, 1])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"CTDA"), condition(0xffff, 0, 0.0, 0));
    let mut game = game(vec![record]);
    assert_eq!(game.purchase_perk(id(FIRST)).unwrap().perk, id(FIRST));
}

#[test]
fn points_and_playability_are_checked_before_spending() {
    let mut game = game(vec![
        Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 0, 0])
    ]);
    let before = state(&game);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Err(Error::NotPlayable(id(FIRST)))
    );
    assert_eq!(state(&game), before);

    // PRG-16: non-default Trait, Level, Num Ranks and Hidden no longer reject
    // a playable, conditionless purchase, see crates/game/src/perks.rs.
    for data in [
        [1, 0, 1, 1, 0],
        [0, 5, 1, 1, 0],
        [0, 0, 2, 1, 0],
        [0, 0, 1, 1, 1],
    ] {
        let mut game = self::game(vec![
            Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), data)
        ]);
        assert_eq!(
            game.purchase_perk(id(FIRST)),
            Ok(Purchase {
                perk: id(FIRST),
                rank: 1
            })
        );
        assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
    }

    let mut game = self::game(vec![perk(FIRST)]);
    for points in [0, -1] {
        game.world_mut().perk_points = points;
        assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::NoPoints));
        assert_eq!(state(&game), (points, BTreeSet::new()));
    }
}

#[test]
fn or_groups_and_unsupported_conditions_preserve_transaction_rules() {
    let record = perk(FIRST)
        .sub(Tag(*b"CTDA"), condition(448, SECOND, 1.0, 1))
        .sub(Tag(*b"CTDA"), condition(448, FIRST, 0.0, 0));
    let mut game = game(vec![record, perk(SECOND)]);
    game.purchase_perk(id(FIRST)).unwrap();
    let record = perk(FIRST).sub(Tag(*b"CTDA"), condition(0xffff, 0, 0.0, 0));
    let mut game = self::game(vec![record]);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Err(Error::Condition(conditions::Error::Function(0xffff)))
    );
    assert_eq!(state(&game), (5, BTreeSet::new()));
}

#[test]
fn winning_override_links_use_the_winning_plugins_master_order() {
    let mut original = Plugin::new(FileHeader::default());
    original.push_top(perk(FIRST));
    let mut other = Plugin::new(FileHeader::default());
    other.push_top(perk(SECOND));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Other.esm".into(), "Perks.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(perk(0x0100_0000 | FIRST).sub(Tag(*b"NNAM"), SECOND.to_le_bytes()));
    patch.push_top(perk(SECOND).sub(Tag(*b"CTDA"), condition(448, 0x0100_0000 | FIRST, 1.0, 0)));
    let mut order = LoadOrder::new();
    order.add("Perks.esm", original).unwrap();
    order.add("Other.esm", other).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    game.world_mut().perk_points = 2;
    game.purchase_perk(id(FIRST)).unwrap();
    let second = GlobalFormId {
        file: 1,
        object: SECOND,
    };
    assert_eq!(game.purchase_perk(id(FIRST)).unwrap().perk, second);
    assert_eq!(state(&game), (0, BTreeSet::from([id(FIRST), second])));
}

#[test]
fn unsupported_actor_value_and_missing_condition_target_are_errors() {
    for (parameter, run_on, error) in [
        (u32::MAX, 0_u32, conditions::Error::ActorValue(u32::MAX)),
        (6, 1, conditions::Error::Target(1)),
    ] {
        let mut bytes = condition(277, parameter, 0.0, 0);
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        let mut game = game(vec![perk(FIRST).sub(Tag(*b"CTDA"), bytes)]);
        assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Condition(error)));
        assert_eq!(state(&game), (5, BTreeSet::new()));
    }
}

#[test]
fn externally_granted_ranks_are_retained_without_charging_them_again() {
    let mut records = chain();
    records[0].subrecords[0].data[2] = 0;
    let mut game = game(records);
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .perks
        .insert(id(SECOND));
    assert_eq!(game.purchase_perk(id(FIRST)).unwrap().rank, 1);
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST), id(SECOND)])));
    assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Complete));
}

#[test]
fn installed_perk_purchase_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping perk survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut game = Game::new(order);
    let mut distribution = BTreeMap::new();
    let mut targets = BTreeSet::new();
    let mut roots = Vec::new();
    {
        let world = game.world();
        for (id, record) in world
            .order()
            .winners_of(Tag(*b"PERK"))
            .filter(|(_, r)| !r.is_deleted())
        {
            let data = record.find(Tag(*b"DATA")).unwrap();
            *distribution.entry(data.to_vec()).or_insert(0_usize) += 1;
            roots.push(id);
            if let Some(bytes) = record.find(Tag(*b"NNAM")) {
                let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                if raw != 0 {
                    targets.insert(world.resolve_from(id, raw).unwrap());
                }
            }
        }
    }
    assert!(roots.len() > 200, "expected the full installed perk corpus");
    roots.retain(|id| !targets.contains(id));
    let player = game.world_mut().player();
    {
        let mut world = game.world_mut();
        world.perk_points = 255;
        let actor = world.actor(player).unwrap();
        for name in [
            "onehanded",
            "twohanded",
            "marksman",
            "block",
            "smithing",
            "heavyarmor",
            "lightarmor",
            "pickpocket",
            "lockpicking",
            "sneak",
            "alchemy",
            "speechcraft",
            "alteration",
            "conjuration",
            "destruction",
            "illusion",
            "restoration",
            "enchanting",
        ] {
            actor.values.set_base(name, 100.0).unwrap();
        }
    }
    let mut results = BTreeMap::new();
    for root in roots {
        let result = game.check_perk_purchase(root);
        if let Err(Error::Perk(_) | Error::Cycle(_) | Error::Format(_)) = &result {
            panic!("invalid installed chain {root}: {result:?}");
        }
        let category = match result {
            Ok(_) => "eligible",
            Err(Error::Complete) => "already owned",
            Err(Error::NotPlayable(_)) => "not playable",
            Err(Error::Ineligible) => "prerequisites false",
            Err(error) => panic!("unexpected installed purchase error for {root}: {error}"),
        };
        *results.entry(category).or_insert(0_usize) += 1;
    }
    let armsman = game.world().form("Armsman00").unwrap();
    game.purchase_perk(armsman).unwrap();
    game.world_mut()
        .set_actor_value(player, "onehanded", 19.0)
        .unwrap();
    let before = state(&game);
    assert_eq!(game.purchase_perk(armsman), Err(Error::Ineligible));
    assert_eq!(state(&game), before);
    game.world_mut()
        .set_actor_value(player, "onehanded", 100.0)
        .unwrap();
    for rank in 2..=5 {
        assert_eq!(game.purchase_perk(armsman).unwrap().rank, rank);
    }
    assert_eq!(game.purchase_perk(armsman), Err(Error::Complete));
    assert_eq!(game.world().perk_points, 250);
    eprintln!("PERK DATA distribution: {distribution:?}");
    eprintln!("PERK root eligibility: {results:?}");
    assert!(
        results.get("eligible").copied().unwrap_or(0) > 10,
        "expected ordinary skill perks to be purchasable"
    );
}
