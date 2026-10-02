// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::ai;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const LIST: u32 = 0x801;
const FIRST: u32 = 0x802;
const SECOND: u32 = 0x803;
const SPELL: u32 = 0x804;
const PERK: u32 = 0x805;
const ITEM: u32 = 0x806;
const SHOUT: u32 = 0x807;
const LEV_SPELL: u32 = 0x808;
const LEV_SPELL_MEMBER: u32 = 0x809;
const OUTFIT: u32 = 0x80a;
const KEYWORD: u32 = 0x80b;
const FACTION: u32 = 0x80c;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(npc::build_record(
        BASE,
        "GuardBase",
        "Guard",
        Config::default(),
        Stats::default(),
    ));
    p.push_top(package::build_record(
        FIRST,
        "First",
        0,
        Schedule::default(),
    ));
    p.push_top(package::build_record(
        SECOND,
        "Second",
        0,
        Schedule::default(),
    ));
    p.push_top(
        Record::new(T(b"FLST"), LIST)
            .zsub(T(b"EDID"), "PackageList")
            .sub(T(b"LNAM"), FIRST.to_le_bytes())
            .sub(T(b"LNAM"), SECOND.to_le_bytes()),
    );
    p.push_top(Record::new(T(b"SPEL"), SPELL).zsub(T(b"EDID"), "GenericSpell"));
    p.push_top(Record::new(T(b"PERK"), PERK).zsub(T(b"EDID"), "GenericPerk"));
    p.push_top(Record::new(T(b"MISC"), ITEM).zsub(T(b"EDID"), "GenericItem"));
    p.push_top(Record::new(T(b"SHOU"), SHOUT).zsub(T(b"EDID"), "GenericShout"));
    p.push_top(
        Record::new(T(b"LVSP"), LEV_SPELL)
            .zsub(T(b"EDID"), "GenericLevSpell")
            .sub(T(b"LVLO"), {
                let mut entry = vec![0; 12];
                entry[4..8].copy_from_slice(&LEV_SPELL_MEMBER.to_le_bytes());
                entry[8..10].copy_from_slice(&1_u16.to_le_bytes());
                entry
            }),
    );
    p.push_top(Record::new(T(b"SPEL"), LEV_SPELL_MEMBER).zsub(T(b"EDID"), "GenericLevSpellMember"));
    p.push_top(Record::new(T(b"OTFT"), OUTFIT).zsub(T(b"EDID"), "GenericOutfit"));
    p.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(T(b"EDID"), "GenericKeyword"));
    p.push_top(Record::new(T(b"FACT"), FACTION).zsub(T(b"EDID"), "GenericFaction"));
    let cell = p.push_record(
        Record::new(T(b"CELL"), CELL)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let actor = p.push_record(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "Guard")
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(CELL));
    temp.entries.push(Entry::Record(actor));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    let mut order = LoadOrder::new();
    order.add("MOD10.esm", p).unwrap();
    Game::new(order)
}

#[test]
fn mod_10_package_formlist_routes_its_members_into_the_default_package_stack() {
    let mut game = game();
    let root = std::env::temp_dir().join(format!("soe-mod10-adversarial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Test_DISTR.ini"),
        "Package = PackageList|GuardBase\n",
    )
    .unwrap();
    let report = game.apply_mod_data(std::slice::from_ref(&root));
    assert_eq!(report.distributions_applied, 1);
    let candidates = ai::candidates(
        &mut game.world_mut(),
        soe_esm::GlobalFormId {
            file: 0,
            object: ACTOR,
        },
    );
    let forms = candidates.iter().map(|c| c.package).collect::<Vec<_>>();
    assert_eq!(
        forms,
        vec![
            soe_esm::GlobalFormId {
                file: 0,
                object: FIRST
            },
            soe_esm::GlobalFormId {
                file: 0,
                object: SECOND
            },
        ]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn mod_10_generic_form_routes_each_supported_schema_type_to_its_consumer() {
    let mut game = game();
    let root = std::env::temp_dir().join(format!("soe-mod10-generic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("Test_DISTR.ini"),
        concat!(
            "Form = GenericSpell|GuardBase\n",
            "Form = GenericPerk|GuardBase\n",
            "Form = GenericItem|GuardBase\n",
            "Form = GenericShout|GuardBase\n",
            "Form = GenericLevSpell|GuardBase\n",
            "Form = GenericOutfit|GuardBase\n",
            "Form = GenericKeyword|GuardBase\n",
            "Form = GenericFaction|GuardBase\n",
            "Form = PackageList|GuardBase\n",
        ),
    )
    .unwrap();
    let report = game.apply_mod_data(std::slice::from_ref(&root));
    assert_eq!(report.distributions_applied, 9);
    assert_eq!(report.skipped_unsupported_consumer, 0);

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        assert!(actor.spells.contains(&id(SPELL)));
        assert!(actor.spells.contains(&id(LEV_SPELL_MEMBER)));
        assert!(actor.perks.contains(&id(PERK)));
        assert!(actor.shouts.contains(&id(SHOUT)));
        assert_eq!(actor.outfit, Some(id(OUTFIT)));
        assert!(actor.factions.contains_key(&id(FACTION)));
    }

    let candidates = ai::candidates(&mut game.world_mut(), id(ACTOR));
    let forms = candidates
        .iter()
        .map(|candidate| candidate.package)
        .collect::<Vec<_>>();
    assert_eq!(forms, vec![id(FIRST), id(SECOND)]);
    assert!(game.world_mut().has_keyword(id(BASE), id(KEYWORD)));
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(ITEM)), 1);
    let _ = std::fs::remove_dir_all(root);
}
