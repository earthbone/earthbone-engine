// SPDX-License-Identifier: GPL-3.0-or-later

//! Console diagnostics fixtures using the existing plugin builders and CTDA
//! evaluator. No test substitutes private saved state for a console operation.

use super::*;
use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    game_with_random_condition(false)
}

fn game_with_random_condition(random: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 0x900)
            .zsub(Tag(*b"EDID"), "TestCell")
            .sub(Tag(*b"DATA"), [1_u8]),
    );
    for base in [7, 0x800] {
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), base)
                .sub(Tag(*b"ACBS"), vec![0; 24])
                .zsub(Tag(*b"FULL"), "Dragonborn"),
        );
    }
    for reference in [0x801_u32, 0x802] {
        let mut record = Record::new(Tag(*b"ACHR"), reference)
            .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]);
        record.placement.cell = Some(0x900);
        plugin.push_top(record);
    }
    plugin.push_top(crate::quest::build_record(0x810, "DebugQuest", 0));
    let package =
        crate::package::build_record(0x880, "DebugTravel", 0, crate::package::Schedule::default())
            .sub(
                Tag(*b"PKCU"),
                [
                    1_u32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                ]
                .concat(),
            );
    let package = crate::package::with_location_input(
        package,
        0,
        crate::package::Location::NearReference(0x802),
        0,
    );
    let package =
        crate::package::with_procedure(crate::package::finish(package), "Travel", &[0], false);
    plugin.push_top(package);
    let topic = Record::new(Tag(*b"DIAL"), 0x820)
        .zsub(Tag(*b"EDID"), "VendorTopic")
        .sub(Tag(*b"QNAM"), 0x810_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), [0; 4]);
    let mut top = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    top.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(0x820));
    let mut true_condition = vec![0; 32];
    true_condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    true_condition[8..10].copy_from_slice(&72_u16.to_le_bytes());
    true_condition[12..16].copy_from_slice(&0x800_u32.to_le_bytes());
    let mut false_condition = true_condition.clone();
    false_condition[4..8].copy_from_slice(&0.0_f32.to_le_bytes());
    let info = Record::new(Tag(*b"INFO"), 0x821)
        .sub(Tag(*b"CTDA"), true_condition)
        .sub(Tag(*b"CTDA"), false_condition);
    let info = if random {
        let mut condition = vec![0; 32];
        condition[8..10].copy_from_slice(&77_u16.to_le_bytes());
        info.sub(Tag(*b"CTDA"), condition)
    } else {
        info
    };
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    top.entries.push(Entry::Group(children));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order.add("Debug.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().load_cell(id(0x900));
    for actor in [0x801, 0x802] {
        let mut world = game.world_mut();
        let state = world.actor(id(actor)).unwrap();
        state.ai.procedure = crate::ai::Procedure::Travel;
        state.ai.destination = Some([100.0, 0.0, 0.0]);
        state.ai.path = vec![[50.0, 0.0, 0.0], [100.0, 0.0, 0.0]];
    }
    let player = game.player();
    game.world_mut().reference(player).unwrap().display_name = Some("Dragonborn".into());
    game.start_quest(id(0x810));
    game
}

macro_rules! toggle_test {
    ($test:ident, $command:literal, $field:ident) => {
        #[test]
        fn $test() {
            let mut game = game();
            let mut console = Console::new();
            assert_eq!(
                console.run(&mut game, $command).unwrap(),
                Outcome::Bool(true)
            );
            assert!(game.debug_views.$field);
            assert_eq!(
                console.run(&mut game, $command).unwrap(),
                Outcome::Bool(false)
            );
        }
    };
}
toggle_test!(togglenavmesh_command, "ToggleNavMesh", navmesh);
toggle_test!(tnm_command, "tnm", navmesh);
toggle_test!(togglepathline_command, "TogglePathLine", paths);
toggle_test!(tpl_command, "tpl", paths);
toggle_test!(toggledebugtext_command, "ToggleDebugText", ai_text);
toggle_test!(tdt_command, "tdt", ai_text);

macro_rules! request_test {
    ($test:ident, $command:literal, $request:expr) => {
        #[test]
        fn $test() {
            let mut game = game();
            let mut console = Console::new();
            console.select(id(0x801));
            console.run(&mut game, $command).unwrap();
            assert_eq!(game.debug_views.host_requests.pop_front(), Some($request));
            assert!(game.debug_views.host_requests.is_empty());
        }
    };
}
request_test!(
    showanim_command,
    "ShowAnim",
    HostRequest::Animation(id(0x801))
);
request_test!(sa_command, "sa", HostRequest::Animation(id(0x801)));
request_test!(
    showscenegraph_command,
    "ShowScenegraph",
    HostRequest::Scenegraph(id(0x801))
);
request_test!(ssg_command, "ssg", HostRequest::Scenegraph(id(0x801)));
request_test!(sfsg_command, "sfsg", HostRequest::Scenegraph(id(0x801)));
request_test!(
    shownodes_command,
    "ShowNodes All",
    HostRequest::Scenegraph(id(0x801))
);
request_test!(togglemapcam_command, "ToggleMapCam", HostRequest::MapCamera);
request_test!(tmc_command, "tmc", HostRequest::MapCamera);
request_test!(togglehdrdebug_command, "ToggleHDRDebug", HostRequest::Hdr);
request_test!(thd_command, "thd", HostRequest::Hdr);
request_test!(
    testlocalmap_command,
    "TestLocalMap 0",
    HostRequest::LocalMap
);
request_test!(tlm_command, "tlm 0", HostRequest::LocalMap);

macro_rules! text_test {
    ($test:ident, $command:literal, $needle:literal) => {
        #[test]
        fn $test() {
            let mut game = game();
            let mut console = Console::new();
            console.select(id(0x801));
            let result = console.run(&mut game, $command).unwrap().to_string();
            assert!(result.contains($needle), "{result}");
        }
    };
}
text_test!(printailist_command, "PrintAiList", "path=found waypoints=2");
text_test!(pai_command, "pai", "procedure=Travel");
text_test!(showailist_command, "ShowAiList", "destination=Some");
text_test!(printnpcdialog_command, "PrintNPCDialog", "result=Ok(false)");
text_test!(pdialog_command, "pdialog", "result=Ok(true)");
text_test!(
    calcpathtopoint_command,
    "CalcPathToPoint 10 10 0",
    "failed:"
);
text_test!(p2p_command, "p2p 10 10 0", "failed:");
text_test!(getplayername_command, "GetPlayerName", "Dragonborn");
text_test!(checkmemory_command, "CheckMemory", "Rust live_bytes=");
text_test!(chkmem_command, "chkmem", "allocation_calls=");
text_test!(
    testseendata_command,
    "TestSeenData",
    "detected_player=false"
);
text_test!(tsd_command, "tsd", "3d_loaded=");

macro_rules! pivot_test {
    ($test:ident, $command:literal) => {
        #[test]
        fn $test() {
            let mut game = game();
            let mut console = Console::new();
            console.select(id(0x801));
            console.run(&mut game, $command).unwrap();
            assert_eq!(game.debug_views.pivot, Some(id(0x801)));
            console.run(&mut game, $command).unwrap();
            assert_eq!(game.debug_views.pivot, None);
        }
    };
}
pivot_test!(showpivot_command, "ShowPivot");
pivot_test!(sp_command, "sp");
pivot_test!(visualrefposition_command, "VisualRefPosition");
pivot_test!(vrp_command, "vrp");

macro_rules! papyrus_test {
    ($test:ident, $command:literal) => {
        #[test]
        fn $test() {
            let mut game = game();
            let directory = std::env::temp_dir().join(concat!("debug-views-", stringify!($test)));
            game.debug_views.papyrus_directory = Some(directory.clone());
            let mut console = Console::new();
            console.run(&mut game, $command).unwrap();
            game.vm.log.push(1.0, "diagnostic trace");
            game.run_until_idle(1);
            assert_eq!(
                papyrus_lines(&mut game),
                ["[Papyrus 1.000] diagnostic trace"]
            );
            assert!(
                std::fs::read_to_string(directory.join("Logs/Script/Papyrus.0.log"))
                    .unwrap()
                    .contains("diagnostic trace")
            );
            console.run(&mut game, $command).unwrap();
            game.vm.log.push(2.0, "hidden overlay trace");
            assert!(papyrus_lines(&mut game).is_empty());
            std::fs::remove_dir_all(directory).unwrap();
        }
    };
}
papyrus_test!(togglepapyruslog_command, "TogglePapyrusLog");
papyrus_test!(tplog_command, "TPLog");

macro_rules! unavailable_test {
    ($test:ident, $command:literal, $reason:literal) => {
        #[test]
        fn $test() {
            let mut game = game();
            assert!(Console::new()
                .run(&mut game, $command)
                .unwrap_err()
                .contains($reason));
        }
    };
}
unavailable_test!(toggleverletdebug_command, "ToggleVerletDebug", "Jolt");
unavailable_test!(tvd_command, "tvd", "Jolt");
unavailable_test!(
    togglemultiboundcheck_command,
    "ToggleMultiboundCheck",
    "Godot"
);
unavailable_test!(mbc_command, "mbc", "Godot");

#[test]
fn showwhodetectsplayer_command() {
    let mut game = game();
    assert_eq!(
        Console::new()
            .run(&mut game, "ShowWhoDetectsPlayer")
            .unwrap(),
        Outcome::List(Vec::new())
    );
}

#[test]
fn swdp_command() {
    let mut game = game();
    assert_eq!(
        Console::new().run(&mut game, "swdp").unwrap(),
        Outcome::List(Vec::new())
    );
}

#[test]
fn views_require_selection_and_validate_arguments_without_queuing() {
    let mut game = game();
    let mut console = Console::new();
    for command in [
        "sa",
        "ssg",
        "pdialog",
        "sp",
        "p2p NaN 0 0",
        "tlm 1",
        "tnm 1",
        "ShowNodes unknown",
    ] {
        assert!(console.run(&mut game, command).is_err(), "{command}");
    }
    assert!(game.debug_views.host_requests.is_empty());
    assert!(!game.debug_views.drawing());
}

#[test]
fn dialogue_report_checks_all_predicates_without_starting_a_conversation() {
    let mut game = game();
    let lines = npc_dialogue(&mut game, id(0x801)).unwrap().join("\n");
    assert!(lines.contains("CTDA[0] function=72"));
    assert!(lines.contains("CTDA[1] function=72"));
    assert!(lines.contains("selected_candidates=[]"));
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn route_report_distinguishes_one_point_partial_and_found() {
    let game = game();
    let mut world = game.world_mut();
    world.actor(id(0x801)).unwrap().ai.path = vec![[50.0, 0.0, 0.0]];
    assert_eq!(path_state(&world, id(0x801)), "one point");
    world
        .actor(id(0x801))
        .unwrap()
        .ai
        .path
        .push([75.0, 0.0, 0.0]);
    assert_eq!(path_state(&world, id(0x801)), "partial");
    world
        .actor(id(0x801))
        .unwrap()
        .ai
        .path
        .push([100.0, 0.0, 0.0]);
    assert_eq!(path_state(&world, id(0x801)), "found");
}

#[test]
fn package_destination_reports_authored_location_before_path_projection() {
    let mut game = game();
    {
        let mut world = game.world_mut();
        world.reference(id(0x802)).unwrap().position = [150.0, 0.0, 0.0];
        let ai = &mut world.actor(id(0x801)).unwrap().ai;
        ai.package = Some(id(0x880));
        ai.tree = Some(crate::ai::TreeState {
            owner: id(0x880),
            branch: 0,
            active: true,
            finished: false,
            stacked: Vec::new(),
            stacked_completed: Vec::new(),
            simultaneous: None,
        });
    }
    let mut console = Console::new();
    console.select(id(0x801));
    let report = console.run(&mut game, "ShowAiList").unwrap().to_string();
    assert!(
        report.contains("destination=Some([100.0, 0.0, 0.0])"),
        "{report}"
    );
    assert!(
        report.contains("package_destination=Some([150.0, 0.0, 0.0])"),
        "{report}"
    );
}

#[test]
fn papyrus_toggle_restores_profile_file_routing() {
    let mut game = game();
    let root = std::env::temp_dir().join(format!("debug-views-profile-{}", std::process::id()));
    let profile = root.join("profile");
    let diagnostic = root.join("diagnostic");
    game.set_papyrus_log_directory(
        &profile,
        crate::PapyrusLogSettings {
            enabled: true,
            trace_enabled: true,
            debug_information: true,
        },
    )
    .unwrap();
    game.debug_views.papyrus_directory = Some(diagnostic.clone());
    let mut console = Console::new();
    console.run(&mut game, "TPLog").unwrap();
    game.vm.log.push(1.0, "diagnostic sink");
    game.run_until_idle(1);
    console.run(&mut game, "TPLog").unwrap();
    game.vm.log.push(2.0, "profile resumed");
    game.run_until_idle(1);
    let profile_log = std::fs::read_to_string(profile.join("Logs/Script/Papyrus.0.log")).unwrap();
    let debug_log = std::fs::read_to_string(diagnostic.join("Logs/Script/Papyrus.0.log")).unwrap();
    assert!(profile_log.contains("profile resumed") && !profile_log.contains("diagnostic sink"));
    assert!(debug_log.contains("diagnostic sink") && !debug_log.contains("profile resumed"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dialogue_random_previews_preserve_the_next_gameplay_draw() {
    let baseline = game();
    let mut game = game_with_random_condition(true);
    game.world_mut().reset_random_seed(42);
    baseline.world_mut().reset_random_seed(42);
    let expected = baseline.world_mut().random_u64();
    let mut console = Console::new();
    console.select(id(0x801));
    let report = console.run(&mut game, "pdialog").unwrap().to_string();
    assert!(report.contains("CTDA[2] function=77"), "{report}");
    assert!(report.contains("random_preview=true"), "{report}");
    assert_eq!(game.world_mut().random_u64(), expected);
}
