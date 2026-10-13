// SPDX-License-Identifier: GPL-3.0-or-later

//! Console commands are exercised through Console::run against authored records.

use super::{Console, Outcome};
use crate::{npc, quest, Game};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn fixture() -> (Game, Console) {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, name) in [(7, "Player"), (0x601, "TestActorBase")] {
        plugin.push_top(npc::build_record(
            object,
            name,
            name,
            npc::Config::default(),
            npc::Stats::default(),
        ));
    }
    plugin.push_top(
        Record::new(T(b"CELL"), 0x700)
            .zsub(T(b"EDID"), "TestCell")
            .sub(T(b"DATA"), [1, 0]),
    );
    for (object, tag, name) in [
        (0xf, b"MISC", "Gold001"),
        (0xb00, b"DIAL", "F"),
        (0x200, b"MISC", "TestItem"),
        (0x300, b"CONT", "TestContainer"),
        (0x400, b"DOOR", "TestDoorBase"),
        (0x901, b"SPEL", "TestSpell"),
        (0x902, b"PERK", "TestPerk"),
        (0x903, b"SHOU", "TestShout"),
        (0x904, b"WOOP", "TestWord"),
        (0x905, b"FACT", "TestFaction"),
        (0x906, b"LCTN", "TestLocation"),
        (0x907, b"IDLE", "TestIdle"),
        (0x908, b"WTHR", "TestWeather"),
        (0x909, b"RACE", "TestRace"),
    ] {
        let mut record = Record::new(T(tag), object)
            .zsub(T(b"EDID"), name)
            .zsub(T(b"FULL"), name);
        if tag == b"CONT" {
            let mut item = 0x200_u32.to_le_bytes().to_vec();
            item.extend_from_slice(&3_i32.to_le_bytes());
            record = record.sub(T(b"CNTO"), item);
        }
        if tag == b"FACT" {
            record = record.sub(T(b"DATA"), [0_u8; 4]);
        }
        plugin.push_top(record);
    }
    for (object, base, actor) in [
        (0x500, 0x400, false),
        (0x501, 0x300, false),
        (0x502, 0x300, false),
        (0x600, 0x601, true),
    ] {
        let mut record = Record::new(T(if actor { b"ACHR" } else { b"REFR" }), object)
            .sub(T(b"NAME"), u32::to_le_bytes(base))
            .sub(T(b"DATA"), [0; 24]);
        record.placement.cell = Some(0x700);
        plugin.push_top(record);
    }
    let quest = quest::with_objective(
        quest::with_stage(
            quest::build_record(0x800, "TestQuest", 0),
            10,
            0,
            0,
            "Test stage",
        ),
        20,
        "Test objective",
    );
    plugin.push_top(quest);
    plugin.push_top(
        Record::new(T(b"GLOB"), 0x910)
            .zsub(T(b"EDID"), "TimeScale")
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 20_f32.to_le_bytes()),
    );
    for (object, name, value) in [
        (0xa00, "fTestSetting", 1.0),
        (0xa01, "fSkillUseCurve", 1.0),
        (0xa02, "fXPLevelUpBase", 100.0),
        (0xa03, "fXPLevelUpMult", 1.0),
    ] {
        plugin.push_top(
            Record::new(T(b"GMST"), object)
                .zsub(T(b"EDID"), name)
                .sub(T(b"DATA"), f32::to_le_bytes(value)),
        );
    }
    let mut skill = Vec::new();
    for value in [1_f32, 0.0, 1.0, 0.0] {
        skill.extend_from_slice(&value.to_le_bytes());
    }
    plugin.push_top(
        Record::new(T(b"AVIF"), 0xa10)
            .zsub(T(b"EDID"), "AVOneHanded")
            .sub(T(b"AVSK"), skill),
    );
    let mut level_entry = vec![0_u8; 12];
    level_entry[..2].copy_from_slice(&1_u16.to_le_bytes());
    level_entry[4..8].copy_from_slice(&0x200_u32.to_le_bytes());
    level_entry[8..10].copy_from_slice(&2_u16.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"LVLI"), 0xa20)
            .zsub(T(b"EDID"), "TestLeveledItems")
            .sub(T(b"LVLF"), [0])
            .sub(T(b"LVLD"), [0])
            .sub(T(b"LLCT"), [1])
            .sub(T(b"LVLO"), level_entry),
    );
    plugin.push_top(
        Record::new(T(b"ARMO"), 0xa21)
            .zsub(T(b"EDID"), "TestArmor")
            .sub(
                T(b"DATA"),
                [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
            )
            .sub(T(b"DNAM"), 100_u32.to_le_bytes())
            .sub(
                T(b"BOD2"),
                [4_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    let mut effect_data = vec![0_u8; 152];
    effect_data[..4].copy_from_slice(&2_u32.to_le_bytes());
    effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24_u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&1_u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"MGEF"), 0xa30)
            .zsub(T(b"EDID"), "TestEffect")
            .sub(T(b"DATA"), effect_data),
    );
    let mut spell_data = vec![0_u8; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let effect_values = [
        7_f32.to_le_bytes(),
        0_u32.to_le_bytes(),
        10_u32.to_le_bytes(),
    ]
    .concat();
    plugin.push_top(
        Record::new(T(b"SPEL"), 0xa31)
            .zsub(T(b"EDID"), "TestEffectSpell")
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), 0xa30_u32.to_le_bytes())
            .sub(T(b"EFIT"), effect_values),
    );
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().reference(player).unwrap().is_actor = true;
    game.world_mut().reference(player).unwrap().cell = Some(id(0x700));
    game.world_mut().actor(player).unwrap();
    game.world_mut().actor(id(0x600)).unwrap();
    game.world_mut().add_item(player, id(0xf), 1000);
    game.world_mut()
        .set_actor_value(player, "health", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(player, "onehanded", 15.0)
        .unwrap();
    let mut console = Console::new();
    console.select(id(0x600));
    (game, console)
}

macro_rules! answer {
    ($name:ident, $line:literal, $expected:expr) => {
        #[test]
        fn $name() {
            let (mut game, mut console) = fixture();
            assert_eq!(console.run(&mut game, $line).unwrap(), $expected);
        }
    };
}

answer!(prid_selects_reference, "prid 500", Outcome::Form(id(0x500)));
answer!(
    getselected_retains_reference,
    "getselected",
    Outcome::Form(id(0x600))
);
answer!(
    getav_reads_value,
    "player.getav health",
    Outcome::Float(100.0)
);
answer!(
    getbaseav_reads_base,
    "player.getbaseav health",
    Outcome::Float(100.0)
);
answer!(
    getactorvaluepercent_reads_fraction,
    "player.getactorvaluepercent health",
    Outcome::Float(1.0)
);
answer!(
    getpermanentactorvalue_reads_total,
    "player.getpermanentactorvalue health",
    Outcome::Float(100.0)
);
answer!(getlevel_reads_actor, "player.getlevel", Outcome::Int(1));
answer!(getscale_reads_reference, "getscale", Outcome::Float(1.0));
answer!(getpos_reads_reference, "getpos x", Outcome::Float(0.0));
answer!(getangle_reads_degrees, "getangle z", Outcome::Float(0.0));
answer!(getself_returns_target, "getself", Outcome::Form(id(0x600)));
answer!(
    getdistance_measures_positions,
    "getdistance player",
    Outcome::Float(0.0)
);
answer!(getdead_reads_lifecycle, "getdead", Outcome::Bool(false));
answer!(isessential_reads_flag, "isessential", Outcome::Bool(false));
answer!(isprotected_reads_flag, "isprotected", Outcome::Bool(false));
answer!(isincombat_reads_flag, "isincombat", Outcome::Bool(false));
answer!(getisghost_reads_flag, "getisghost", Outcome::Bool(false));
answer!(getlocked_reads_lock, "500.getlocked", Outcome::Bool(false));
answer!(
    getlocklevel_reads_difficulty,
    "500.getlocklevel",
    Outcome::Int(0)
);
answer!(
    getdisabled_reads_enable_state,
    "getdisabled",
    Outcome::Bool(false)
);
answer!(
    getfactionrank_reads_absent_membership,
    "getfactionrank TestFaction",
    Outcome::Int(-1)
);
answer!(
    getrelationshiprank_reads_absent_override,
    "getrelationshiprank player",
    Outcome::Int(0)
);
answer!(
    getcrimegold_reads_faction_ledger,
    "getcrimegold TestFaction",
    Outcome::Int(0)
);
answer!(
    getlocationcleared_reads_state,
    "getlocationcleared TestLocation",
    Outcome::Bool(false)
);
answer!(
    hasspell_reads_spellbook,
    "hasspell TestSpell",
    Outcome::Bool(false)
);
answer!(
    hasperk_reads_perks,
    "hasperk TestPerk",
    Outcome::Bool(false)
);
answer!(
    hasshout_reads_shoutbook,
    "hasshout TestShout",
    Outcome::Bool(false)
);
answer!(
    coc_moves_player_cell,
    "COC TestCell",
    Outcome::Form(id(0x700))
);
answer!(
    getpcmiscstat_reads_missing_stat,
    "GetPCMS \"days as a werewolf\"",
    Outcome::Int(0)
);
answer!(
    modpcmiscstat_changes_stat,
    "ModPCMS \"days as a werewolf\" 3",
    Outcome::Int(3)
);

#[test]
fn quotes_commas_comments_and_player_prefix_preserve_selection() {
    let (mut game, mut console) = fixture();
    console
        .run(&mut game, "\"00000500\".lock 25 ; comment")
        .unwrap();
    console
        .run(&mut game, "player.SetActorValue Health, 250")
        .unwrap();
    assert_eq!(console.selected(), Some(id(0x600)));
    assert!(game.world().reference_ref(id(0x500)).unwrap().locked);
    assert_eq!(
        console
            .run(&mut game, "player.GetActorValue health")
            .unwrap(),
        Outcome::Float(250.0)
    );
}

#[test]
fn invalid_number_and_quote_leave_state_unchanged() {
    let (mut game, mut console) = fixture();
    for line in [
        "player.setav health NaN",
        "setpos y inf",
        "setscale -1",
        "prid \"500",
        "player.additem TestItem 1.5",
    ] {
        assert!(console.run(&mut game, line).is_err(), "{line}");
    }
    let player = game.player();
    assert_eq!(game.world_mut().actor_value(player, "health"), 100.0);
}

#[test]
fn markfordelete_waits_for_cell_reload_and_survives_save() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "500.markfordelete").unwrap();
    let mut world = game.world_mut();
    assert!(!world.reference(id(0x500)).unwrap().deleted);
    let bytes = world.save_state();
    world.load_state(&bytes).unwrap();
    assert!(world.reference(id(0x500)).unwrap().console_delete_pending);
    world.load_cell(id(0x700));
    assert!(world.reference(id(0x500)).unwrap().deleted);
}

macro_rules! changes {
    ($name:ident, $line:literal, $query:literal, $expected:expr) => {
        #[test]
        fn $name() {
            let (mut game, mut console) = fixture();
            console.run_block(&mut game, $line).unwrap();
            assert_eq!(console.run(&mut game, $query).unwrap(), $expected);
        }
    };
}
changes!(
    setav_changes_shared_state,
    "player.setav health 120",
    "player.getbaseav health",
    Outcome::Float(120.0)
);
changes!(
    modav_changes_shared_state,
    "player.modav health 20",
    "player.getav health",
    Outcome::Float(120.0)
);
changes!(
    forceav_changes_shared_state,
    "player.forceav health 150",
    "player.getav health",
    Outcome::Float(150.0)
);
changes!(
    damageactorvalue_changes_shared_state,
    "player.damageactorvalue health 30",
    "player.getav health",
    Outcome::Float(70.0)
);
changes!(
    restoreactorvalue_changes_shared_state,
    "player.damageactorvalue health 30\nplayer.restoreactorvalue health 30",
    "player.getav health",
    Outcome::Float(100.0)
);
changes!(
    resethealth_changes_shared_state,
    "player.damageactorvalue health 70\nplayer.resethealth",
    "player.getav health",
    Outcome::Float(100.0)
);
changes!(
    additem_changes_shared_state,
    "player.additem TestItem 3",
    "player.getitemcount TestItem",
    Outcome::Int(3)
);
changes!(
    removeitem_changes_shared_state,
    "player.additem TestItem 5\nplayer.removeitem TestItem 2",
    "player.getitemcount TestItem",
    Outcome::Int(3)
);
changes!(
    removeallitems_changes_shared_state,
    "player.removeallitems",
    "player.getitemcount Gold001",
    Outcome::Int(0)
);
changes!(
    duplicateallitems_changes_shared_state,
    "501.duplicateallitems 502",
    "502.getitemcount TestItem",
    Outcome::Int(6)
);
changes!(
    drop_changes_shared_state,
    "player.additem TestItem 5\nplayer.drop TestItem 2",
    "player.getitemcount TestItem",
    Outcome::Int(3)
);
changes!(
    resetinventory_changes_shared_state,
    "501.removeallitems\n501.resetinventory",
    "501.getitemcount TestItem",
    Outcome::Int(3)
);
changes!(
    lock_changes_shared_state,
    "500.lock 25",
    "500.getlocked",
    Outcome::Bool(true)
);
changes!(
    unlock_changes_shared_state,
    "500.lock 255\n500.unlock",
    "500.getlocked",
    Outcome::Bool(false)
);
changes!(
    disable_changes_shared_state,
    "disable",
    "getdisabled",
    Outcome::Bool(true)
);
changes!(
    enable_changes_shared_state,
    "disable\nenable",
    "getdisabled",
    Outcome::Bool(false)
);
changes!(
    setpos_changes_shared_state,
    "setpos x 250",
    "getpos x",
    Outcome::Float(250.0)
);
changes!(
    setangle_changes_shared_state,
    "setangle z 90",
    "getangle z",
    Outcome::Float(90.0)
);
changes!(
    setscale_changes_shared_state,
    "setscale 2",
    "getscale",
    Outcome::Float(2.0)
);
changes!(
    modscale_changes_shared_state,
    "modscale 1",
    "getscale",
    Outcome::Float(2.0)
);
changes!(
    setghost_changes_shared_state,
    "setghost 1",
    "getisghost",
    Outcome::Bool(true)
);
changes!(
    setessential_changes_shared_state,
    "setessential TestActorBase 1",
    "isessential",
    Outcome::Bool(true)
);
changes!(
    setprotected_changes_shared_state,
    "setprotected TestActorBase 1",
    "isprotected",
    Outcome::Bool(true)
);
changes!(
    setlevel_changes_shared_state,
    "setlevel 5",
    "getlevel",
    Outcome::Int(5)
);
changes!(
    advlevel_changes_shared_state,
    "player.advlevel",
    "player.getlevel",
    Outcome::Int(2)
);
changes!(
    addspell_changes_shared_state,
    "addspell TestSpell",
    "hasspell TestSpell",
    Outcome::Bool(true)
);
changes!(
    removespell_changes_shared_state,
    "addspell TestSpell\nremovespell TestSpell",
    "hasspell TestSpell",
    Outcome::Bool(false)
);
changes!(
    addperk_changes_shared_state,
    "addperk TestPerk",
    "hasperk TestPerk",
    Outcome::Bool(true)
);
changes!(
    removeperk_changes_shared_state,
    "addperk TestPerk\nremoveperk TestPerk",
    "hasperk TestPerk",
    Outcome::Bool(false)
);
changes!(
    addshout_changes_shared_state,
    "addshout TestShout",
    "hasshout TestShout",
    Outcome::Bool(true)
);
changes!(
    removeshout_changes_shared_state,
    "addshout TestShout\nremoveshout TestShout",
    "hasshout TestShout",
    Outcome::Bool(false)
);
changes!(
    addtofaction_changes_shared_state,
    "addfac TestFaction 3",
    "getfactionrank TestFaction",
    Outcome::Int(3)
);
changes!(
    setfactionrank_changes_shared_state,
    "setfactionrank TestFaction 2",
    "getfactionrank TestFaction",
    Outcome::Int(2)
);
changes!(
    removefromfaction_changes_shared_state,
    "addfac TestFaction 0\nremovefac TestFaction",
    "getfactionrank TestFaction",
    Outcome::Int(-1)
);
changes!(
    removefromallfactions_changes_shared_state,
    "addfac TestFaction 0\nremovefromallfactions",
    "getfactionrank TestFaction",
    Outcome::Int(-1)
);
changes!(
    setrelationshiprank_changes_shared_state,
    "setrelationshiprank player 4",
    "getrelationshiprank player",
    Outcome::Int(4)
);
changes!(
    setcrimegold_changes_shared_state,
    "setcrimegold 50 TestFaction",
    "getcrimegold TestFaction",
    Outcome::Int(50)
);
changes!(
    modcrimegold_changes_shared_state,
    "modcrimegold 25 TestFaction",
    "getcrimegold TestFaction",
    Outcome::Int(25)
);
changes!(
    paycrimegold_changes_shared_state,
    "setcrimegold 50 TestFaction\npaycrimegold 0 0 TestFaction",
    "player.getitemcount Gold001",
    Outcome::Int(950)
);
changes!(
    setlocationcleared_changes_shared_state,
    "setlocationcleared TestLocation 1",
    "getlocationcleared TestLocation",
    Outcome::Bool(true)
);
changes!(
    setstage_changes_shared_state,
    "setstage TestQuest 10",
    "getstage TestQuest",
    Outcome::Int(10)
);
changes!(
    getstagedone_changes_shared_state,
    "setstage TestQuest 10",
    "getstagedone TestQuest 10",
    Outcome::Bool(true)
);
changes!(
    startquest_changes_shared_state,
    "startquest TestQuest",
    "getquestrunning TestQuest",
    Outcome::Bool(true)
);
changes!(
    stopquest_changes_shared_state,
    "startquest TestQuest\nstopquest TestQuest",
    "getquestrunning TestQuest",
    Outcome::Bool(false)
);
changes!(
    completequest_changes_shared_state,
    "completequest TestQuest",
    "getquestcompleted TestQuest",
    Outcome::Bool(true)
);
changes!(
    resetquest_changes_shared_state,
    "setstage TestQuest 10\nresetquest TestQuest",
    "getstage TestQuest",
    Outcome::Int(0)
);
changes!(
    setobjectivedisplayed_changes_shared_state,
    "setobjectivedisplayed TestQuest 20 1",
    "getobjectivedisplayed TestQuest 20",
    Outcome::Bool(true)
);
changes!(
    setobjectivecompleted_changes_shared_state,
    "setobjectivecompleted TestQuest 20 1",
    "getobjectivecompleted TestQuest 20",
    Outcome::Bool(true)
);
changes!(
    setobjectivefailed_changes_shared_state,
    "setobjectivefailed TestQuest 20 1",
    "getobjectivefailed TestQuest 20",
    Outcome::Bool(true)
);
changes!(
    completeallobjectives_changes_shared_state,
    "completeallobjectives TestQuest",
    "getobjectivecompleted TestQuest 20",
    Outcome::Bool(true)
);
changes!(
    failallobjectives_changes_shared_state,
    "failallobjectives TestQuest",
    "getobjectivefailed TestQuest 20",
    Outcome::Bool(true)
);
changes!(
    saq_changes_shared_state,
    "saq",
    "getquestrunning TestQuest",
    Outcome::Bool(true)
);
changes!(
    caqs_changes_shared_state,
    "caqs",
    "getstagedone TestQuest 10",
    Outcome::Bool(true)
);
changes!(
    moveto_changes_shared_state,
    "600.setpos y 20\nplayer.moveto 600",
    "player.getpos y",
    Outcome::Float(20.0)
);
changes!(
    movetocell_changes_shared_state,
    "movetocell TestCell",
    "getcell",
    Outcome::Form(id(0x700))
);
changes!(
    setgs_changes_shared_state,
    "setgs fTestSetting 2",
    "getgs fTestSetting",
    Outcome::Text("Float(2.0)".into())
);
changes!(
    setinisetting_changes_shared_state,
    "SetINI \"fWorldFOV:Display\" 90",
    "GetINI \"fWorldFOV:Display\"",
    Outcome::Text("90".into())
);
changes!(
    set_changes_shared_state,
    "set TimeScale to 1",
    "getglobalvalue TimeScale",
    Outcome::Float(1.0)
);
changes!(
    advskill_changes_shared_state,
    "player.advskill onehanded 15",
    "player.getbaseav onehanded",
    Outcome::Float(16.0)
);
changes!(
    incpcs_changes_shared_state,
    "incpcs onehanded",
    "player.getbaseav onehanded",
    Outcome::Float(16.0)
);

#[test]
fn tgm_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_god_mode;
    console.run(&mut game, "tgm").unwrap();
    assert_eq!(game.world().debug_god_mode, !before);
    console.run(&mut game, "tgm").unwrap();
    assert_eq!(game.world().debug_god_mode, before);
}

#[test]
fn tim_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_immortal_mode;
    console.run(&mut game, "tim").unwrap();
    assert_eq!(game.world().debug_immortal_mode, !before);
    console.run(&mut game, "tim").unwrap();
    assert_eq!(game.world().debug_immortal_mode, before);
}

#[test]
fn tai_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_ai_enabled;
    console.run(&mut game, "tai").unwrap();
    assert_eq!(game.world().debug_ai_enabled, !before);
    console.run(&mut game, "tai").unwrap();
    assert_eq!(game.world().debug_ai_enabled, before);
}

#[test]
fn tcai_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_combat_ai_enabled;
    console.run(&mut game, "tcai").unwrap();
    assert_eq!(game.world().debug_combat_ai_enabled, !before);
    console.run(&mut game, "tcai").unwrap();
    assert_eq!(game.world().debug_combat_ai_enabled, before);
}

#[test]
fn tcl_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_collisions_enabled;
    console.run(&mut game, "tcl").unwrap();
    assert_eq!(game.world().debug_collisions_enabled, !before);
    console.run(&mut game, "tcl").unwrap();
    assert_eq!(game.world().debug_collisions_enabled, before);
}

#[test]
fn tdetect_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_detection_enabled;
    console.run(&mut game, "tdetect").unwrap();
    assert_eq!(game.world().debug_detection_enabled, !before);
    console.run(&mut game, "tdetect").unwrap();
    assert_eq!(game.world().debug_detection_enabled, before);
}

#[test]
fn tscr_toggles_runtime_switch() {
    let (mut game, mut console) = fixture();
    console.deselect();
    let before = game.world().debug_scripts_enabled;
    console.run(&mut game, "tscr").unwrap();
    assert_eq!(game.world().debug_scripts_enabled, !before);
    console.run(&mut game, "tscr").unwrap();
    assert_eq!(game.world().debug_scripts_enabled, before);
}

#[test]
fn setunconscious_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setunconscious 1").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().unconscious);
}

#[test]
fn setrestrained_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setrestrained 1").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().restrained);
}

#[test]
fn setactorsai_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setactorsai 0").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().ai_disabled);
}

#[test]
fn setplayerteammate_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setplayerteammate 1").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().teammate);
}

#[test]
fn setalert_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setalert 1").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().alerted);
}

#[test]
fn equipshout_mutates_actor_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "equipshout TestShout").unwrap();
    assert_eq!(
        game.world().actor_ref(id(0x600)).unwrap().equipped_shout,
        Some(id(0x903))
    );
}

#[test]
fn stopcombat_clears_engagement() {
    let (mut game, mut console) = fixture();
    game.world_mut().actor(id(0x600)).unwrap().in_combat = true;
    console.run(&mut game, "stopcombat").unwrap();
    assert!(!game.world().actor_ref(id(0x600)).unwrap().in_combat);
}

#[test]
fn resetai_clears_engagement() {
    let (mut game, mut console) = fixture();
    game.world_mut().actor(id(0x600)).unwrap().in_combat = true;
    console.run(&mut game, "resetai").unwrap();
    assert!(!game.world().actor_ref(id(0x600)).unwrap().in_combat);
}

#[test]
fn stopcombatalarmonactor_clears_engagement() {
    let (mut game, mut console) = fixture();
    game.world_mut().actor(id(0x600)).unwrap().in_combat = true;
    console.run(&mut game, "stopcombatalarmonactor").unwrap();
    assert!(!game.world().actor_ref(id(0x600)).unwrap().in_combat);
}

#[test]
fn fov_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "fov 90").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Fov(90.0)]
    );
}

#[test]
fn sgtm_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "SGTM 0.5").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::TimeMultiplier(0.5)]
    );
}

#[test]
fn save_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "save \"Test save\"").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Save {
            name: "Test save".into(),
            load: false
        }]
    );
}

#[test]
fn load_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "load \"Test save\"").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Save {
            name: "Test save".into(),
            load: true
        }]
    );
}

#[test]
fn showracemenu_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "showracemenu").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::RaceMenu]
    );
}

#[test]
fn openactorcontainer_queues_host_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "openactorcontainer 1").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Container(id(0x600))]
    );
}

#[test]
fn psb_adds_spells_shouts_and_teaches_words_without_unlocking_them() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "psb").unwrap();
    let player = game.player();
    let world = game.world();
    let actor = world.actor_ref(player).unwrap();
    assert!(actor.spells.contains(&id(0x901)) && actor.shouts.contains(&id(0x903)));
    assert!(world.words_taught.contains(&id(0x904)) && world.words_unlocked.is_empty());
}

#[test]
fn teachword_adds_only_taught_word() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "teachword TestWord").unwrap();
    assert!(
        game.world().words_taught.contains(&id(0x904)) && game.world().words_unlocked.is_empty()
    );
}

#[test]
fn unlockword_adds_unlocked_word() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "unlockword TestWord").unwrap();
    assert!(game.world().words_unlocked.contains(&id(0x904)));
}

#[test]
fn sendanimevent_queues_graph_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "sae IdleStop").unwrap();
    assert!(!game.world().animation_requests.is_empty());
}

#[test]
fn playidle_queues_record_named_animation() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "playidle TestIdle").unwrap();
    assert!(!game.world().animation_requests.is_empty());
}

#[test]
fn help_searches_display_names_and_reports_runtime_ids() {
    let (mut game, mut console) = fixture();
    let Outcome::List(found) = console.run(&mut game, "help \"TestItem\" 4").unwrap() else {
        panic!("not a list");
    };
    assert!(found
        .iter()
        .any(|line| line.contains("00000200") && line.contains("TestItem")));
}

#[test]
fn bat_continues_after_errors_and_accepts_vanilla_comments() {
    let (mut game, mut console) = fixture();
    let root = std::env::temp_dir().join(format!("soe-console-bat-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Data")).unwrap();
    std::fs::write(
        root.join("Data/batch.txt"),
        "; first comment\nplayer.additem TestItem 2\nmissingcommand\nplayer.additem TestItem 3\n",
    )
    .unwrap();
    console.set_batch_directory(root.join("Data"));
    let result = console.run(&mut game, "bat batch").unwrap().to_string();
    assert!(result.contains("3: error:"));
    assert_eq!(
        console
            .run(&mut game, "player.getitemcount TestItem")
            .unwrap(),
        Outcome::Int(5)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn tmove_toggles_only_movement_control() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "tmove").unwrap();
    assert!(!game.world().player_control_flags[0]);
    assert!(game.world().player_control_flags[1]);
}

#[test]
fn disableplayercontrols_and_enableplayercontrols_change_requested_flags() {
    let (mut game, mut console) = fixture();
    console
        .run(&mut game, "disableplayercontrols 1 0 0 1 0 0 0 0")
        .unwrap();
    assert!(!game.world().player_control_flags[0] && !game.world().player_control_flags[3]);
    console.run(&mut game, "enableplayercontrols").unwrap();
    assert!(game.world().player_control_flags.iter().all(|flag| *flag));
}

#[test]
fn sqs_lists_authored_stages_and_completion() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "setstage TestQuest 10").unwrap();
    assert_eq!(
        console.run(&mut game, "sqs TestQuest").unwrap(),
        Outcome::List(vec!["Stage 10: 1".into()])
    );
}

#[test]
fn sqo_lists_objective_state() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "startquest TestQuest").unwrap();
    console
        .run(&mut game, "setobjectivedisplayed TestQuest 20 1")
        .unwrap();
    assert!(console
        .run(&mut game, "sqo")
        .unwrap()
        .to_string()
        .contains("TestQuest 20: displayed=true"));
}

#[test]
fn sqt_has_no_targets_for_empty_authored_target_list() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "startquest TestQuest").unwrap();
    assert_eq!(
        console.run(&mut game, "sqt").unwrap(),
        Outcome::List(Vec::new())
    );
}

#[test]
fn fw_forces_and_sw_requests_weather() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "fw TestWeather,1").unwrap();
    let request = game.world().weather_request().unwrap();
    assert!(request.accelerate && request.override_weather);
    console.run(&mut game, "sw TestWeather").unwrap();
    assert!(!game.world().weather_request().unwrap().accelerate);
}

#[test]
fn releaseweatheroverride_clears_override_request() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "fw TestWeather,1").unwrap();
    console.run(&mut game, "rwo").unwrap();
    assert!(game.world().weather_request().is_none());
}

#[test]
fn quitgame_requests_host_exit_without_terminating_test() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "qqq").unwrap();
    assert!(matches!(
        game.world().debug_host_requests.last(),
        Some(crate::world::DebugHostRequest::Quit)
    ));
}

#[test]
fn additem_resolves_leveled_lists_at_runtime_level() {
    let (mut game, mut console) = fixture();
    console
        .run(&mut game, "player.additem TestLeveledItems 3")
        .unwrap();
    assert_eq!(
        console
            .run(&mut game, "player.getitemcount TestItem")
            .unwrap(),
        Outcome::Int(6)
    );
}

#[test]
fn additem_negative_count_removes_items() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "player.additem TestItem 5").unwrap();
    console
        .run(&mut game, "player.additem TestItem -2")
        .unwrap();
    assert_eq!(
        console
            .run(&mut game, "player.getitemcount TestItem")
            .unwrap(),
        Outcome::Int(3)
    );
}

#[test]
fn equipitem_creates_and_protects_equipment() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "equipitem TestArmor 1").unwrap();
    let world = game.world();
    let state = world.actor_ref(id(0x600)).unwrap();
    assert!(
        state.equipped.contains(&id(0xa21)) && state.prevent_equipment_removal.contains(&id(0xa21))
    );
    assert_eq!(
        world
            .reference_ref(id(0x600))
            .unwrap()
            .inventory
            .count(id(0xa21)),
        1
    );
}

#[test]
fn unequipitem_keeps_item_in_inventory() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "equipitem TestArmor").unwrap();
    console.run(&mut game, "unequipitem TestArmor").unwrap();
    let world = game.world();
    assert!(!world
        .actor_ref(id(0x600))
        .unwrap()
        .equipped
        .contains(&id(0xa21)));
    assert_eq!(
        world
            .reference_ref(id(0x600))
            .unwrap()
            .inventory
            .count(id(0xa21)),
        1
    );
}

#[test]
fn setownership_defaults_to_player_base() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "500.setownership").unwrap();
    assert_eq!(
        game.world().reference_ref(id(0x500)).unwrap().owner,
        Some(id(7))
    );
}

#[test]
fn clearownership_removes_reference_owner() {
    let (mut game, mut console) = fixture();
    console
        .run(&mut game, "500.setownership TestFaction")
        .unwrap();
    console.run(&mut game, "500.clearownership").unwrap();
    assert_eq!(game.world().reference_ref(id(0x500)).unwrap().owner, None);
}

#[test]
fn movetoqt_moves_to_current_conditional_objective_target() {
    let (mut game, mut console) = fixture();
    {
        let mut world = game.world_mut();
        let q = world.quest_state(id(0x800)).unwrap();
        q.def.objectives[0].targets.push(quest::ObjectiveTarget {
            alias: 0,
            compass_marker_ignores_locks: false,
            conditions: Vec::new(),
        });
        q.aliases.insert(0, Some(id(0x600)));
        q.objectives.entry(20).or_default().displayed = true;
        world.reference(id(0x600)).unwrap().position = [50.0, 60.0, 70.0];
    }
    console.run(&mut game, "movetoqt TestQuest").unwrap();
    let player = game.player();
    assert_eq!(
        game.world().reference_ref(player).unwrap().position,
        [50.0, 60.0, 70.0]
    );
}

#[test]
fn kill_and_resurrect_reach_actor_lifecycle() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "kill").unwrap();
    assert!(game.world().actor_ref(id(0x600)).unwrap().dead);
    console.run(&mut game, "resurrect 1").unwrap();
    assert!(!game.world().actor_ref(id(0x600)).unwrap().dead);
}

#[test]
fn tim_prevents_player_death_at_zero_health() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "tim").unwrap();
    console
        .run(&mut game, "player.damageactorvalue health 1000")
        .unwrap();
    let player = game.player();
    assert!(!game.world().actor_ref(player).unwrap().dead);
}

#[test]
fn targeted_tai_disables_only_selected_actor() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "tai").unwrap();
    assert!(
        game.world().debug_ai_enabled && game.world().actor_ref(id(0x600)).unwrap().ai_disabled
    );
}

#[test]
fn tfc_and_sucsm_queue_free_camera_parameters() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "tfc 1").unwrap();
    console.run(&mut game, "sucsm 50").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![
            super::HostRequest::FreeCamera(true),
            super::HostRequest::FreeCameraSpeed(50.0)
        ]
    );
}

#[test]
fn cast_applies_record_effect_without_learning_the_spell() {
    let (mut game, mut console) = fixture();
    console
        .run(&mut game, "player.cast TestEffectSpell 600 right")
        .unwrap();
    let player = game.player();
    let world = game.world();
    assert!(world
        .active_effects()
        .any(|effect| effect.target == id(0x600)));
    assert!(!world.actor_ref(player).unwrap().spells.contains(&id(0xa31)));
    assert_eq!(
        world.actor_ref(player).unwrap().right_spell,
        Some(id(0xa31))
    );
}

#[test]
fn dispelallspells_removes_effects_and_preserves_known_spells() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "addspell TestSpell").unwrap();
    console
        .run(&mut game, "player.cast TestEffectSpell 600 right")
        .unwrap();
    console.run(&mut game, "dispelallspells").unwrap();
    assert!(!game
        .world()
        .active_effects()
        .any(|effect| effect.target == id(0x600)));
    assert!(game
        .world()
        .actor_ref(id(0x600))
        .unwrap()
        .spells
        .contains(&id(0x901)));
}

#[test]
fn pushactoraway_queues_signed_actor_impulse() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "pushactoraway player 10").unwrap();
    let player = game.player();
    assert!(!game.world_mut().take_actor_impulses(player).is_empty());
}

#[test]
fn csb_queues_blood_reset() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "csb").unwrap();
    assert_eq!(
        console.take_host_requests(),
        vec![super::HostRequest::Toggle("csb".into())]
    );
}

#[test]
fn duplicateallitems_preserves_condition_without_aliasing_reference_identity() {
    use crate::inventory::{Extra, Field, Stack};
    let (mut game, mut console) = fixture();
    let player = game.player();
    let original = game.world_mut().place_at(id(0x200), player, false);
    let runtime_id = game.world().runtime_id(original);
    console
        .run(&mut game, &format!("player.additem {runtime_id:08x} 1"))
        .unwrap();
    let stack = Stack::new(
        1,
        vec![
            Extra {
                kind: 32,
                fields: vec![Field::Form(Some(original))],
            },
            Extra {
                kind: 37,
                fields: vec![Field::FloatBits(0.5f32.to_bits())],
            },
        ],
    )
    .unwrap();
    game.world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .restore(id(0x200), 1, vec![stack])
        .unwrap();
    console
        .run(&mut game, "player.duplicateallitems 502")
        .unwrap();
    let mut world = game.world_mut();
    let copied = world
        .reference(id(0x502))
        .unwrap()
        .inventory
        .stacks(id(0x200));
    assert!(copied.iter().all(|stack| stack.reference().is_none()));
    assert!(copied.iter().any(|stack| stack.extra(37).is_some()));
    assert_eq!(
        world.reference(player).unwrap().inventory.stacks(id(0x200))[0].reference(),
        Some(original)
    );
    let save = world.save_state();
    world.load_state(&save).unwrap();
}

#[test]
fn drop_reuses_concrete_reference_identity_and_saves_the_loose_item() {
    let (mut game, mut console) = fixture();
    let player = game.player();
    let original = game.world_mut().place_at(id(0x200), player, false);
    let runtime_id = game.world().runtime_id(original);
    console
        .run(&mut game, &format!("player.additem {runtime_id:08x} 1"))
        .unwrap();
    console.run(&mut game, "player.drop TestItem 1").unwrap();
    let mut world = game.world_mut();
    let reference = world.reference(original).unwrap();
    assert!(reference.enabled);
    assert_eq!(reference.item.as_ref().unwrap().reference(), Some(original));
    assert_eq!(world.item_count(player, id(0x200)), 0);
    let save = world.save_state();
    world.load_state(&save).unwrap();
    assert_eq!(
        world
            .reference(original)
            .unwrap()
            .item
            .as_ref()
            .unwrap()
            .reference(),
        Some(original)
    );
}

#[test]
fn drop_clears_equipment_when_the_last_item_leaves_inventory() {
    let (mut game, mut console) = fixture();
    console
        .run_block(
            &mut game,
            "player.additem TestArmor 1\nplayer.equipitem TestArmor\nplayer.drop TestArmor 1",
        )
        .unwrap();
    let player = game.player();
    assert!(!game
        .world()
        .actor_ref(player)
        .unwrap()
        .equipped
        .contains(&id(0xa21)));
}

#[test]
fn short_form_id_wins_over_the_vanilla_f_editor_id_collision() {
    let (mut game, mut console) = fixture();
    console.run(&mut game, "player.additem f 7").unwrap();
    assert_eq!(
        console
            .run(&mut game, "player.getitemcount 0000000f")
            .unwrap(),
        Outcome::Int(1007)
    );
}

#[test]
fn quest_cell_and_global_name_arguments_keep_hex_shaped_editor_ids() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest::with_stage(
        quest::build_record(0x800, "DA05", 0),
        10,
        0,
        0,
        "Stage",
    ));
    plugin.push_top(
        Record::new(T(b"CELL"), 0x700)
            .zsub(T(b"EDID"), "DAD")
            .sub(T(b"DATA"), [1, 0]),
    );
    plugin.push_top(
        Record::new(T(b"GLOB"), 0x900)
            .zsub(T(b"EDID"), "CAFE")
            .sub(T(b"TNAM"), [b'f'])
            .sub(T(b"FLTV"), 1_f32.to_le_bytes()),
    );
    for object in [0xda05, 0xdad, 0xcafe] {
        plugin.push_top(Record::new(T(b"MISC"), object));
    }
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let mut console = Console::new();
    console.run(&mut game, "setstage DA05 10").unwrap();
    assert_eq!(
        console.run(&mut game, "getstage DA05").unwrap(),
        Outcome::Int(10)
    );
    console.run(&mut game, "sqs DA05").unwrap();
    console.run(&mut game, "set CAFE to 2").unwrap();
    assert_eq!(
        console.run(&mut game, "getglobalvalue CAFE").unwrap(),
        Outcome::Float(2.0)
    );
    console.run(&mut game, "coc DAD").unwrap();
    let player = game.player();
    assert_eq!(
        game.world().reference_ref(player).unwrap().cell,
        Some(id(0x700))
    );
}
