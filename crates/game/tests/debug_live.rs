// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The debug command layer against the installed game. These are the tests
//! that used to need a synthetic plugin, because the state they check sits
//! behind dialogue or a player choice that a cold `Game::start` never
//! reaches. A debug command forces the state instead, so the run is over
//! vanilla records and vanilla scripts.
//!
//! Gated on `SKYRIM_DATA` pointing at the game's `Data` folder:
//!
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-game --test debug_live -- --nocapture
//! ```

use crate::common::installed;

use std::path::PathBuf;

use soe_esm::{GlobalFormId, Tag};
use soe_game::Game;
use soe_game::debug::{Console, Outcome};
use soe_game::world::Handle;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

/// The five masters with their scripts loaded and bound, started. `None`
/// when `SKYRIM_DATA` is unset, which is how every gated test skips.
fn installed_game() -> Option<Game> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    let order = installed::load_order(&MASTERS).unwrap();
    let mut game = Game::from_shared_order(order);
    game.load_scripts_from_data(&data);
    game.start();
    Some(game)
}

/// Actor references the bind step attached a script to, where one of those
/// scripts defines `event`. Only an object the bind already created is
/// considered, so nothing is instantiated just to look at it.
fn actors_handling(game: &mut Game, event: &str) -> Vec<(GlobalFormId, String)> {
    let handlers = game.vm.scripts_handling(event);
    if handlers.is_empty() {
        return Vec::new();
    }
    let actors: Vec<GlobalFormId> = game
        .world()
        .order()
        .winners_of(Tag::new(b"ACHR"))
        .map(|(id, _)| id)
        .collect();
    let mut out = Vec::new();
    for id in actors {
        let Some(object) = game.world().object_id(Handle::Form(id)) else {
            continue;
        };
        for script in game.vm.scripts_on(object) {
            if handlers.iter().any(|h| h.eq_ignore_ascii_case(&script)) {
                out.push((id, script));
                break;
            }
        }
    }
    out
}

/// `SendVampirismStateChanged` "tells anyone who cares that the vampirism
/// state of this actor has changed" (`Actor_Script.wikitext`), which the
/// engine delivers as `OnVampirismStateChanged` to the scripts on that
/// actor. The turn itself runs through dialogue and a player choice, so a
/// cold start never reaches it and the check used to run on a synthetic
/// actor. `setvampire` forces the same call on a vanilla actor instead.
#[test]
fn the_vampire_turn_runs_on_the_installed_game() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let handlers = game.vm.scripts_handling("OnVampirismStateChanged");
    eprintln!(
        "scripts defining OnVampirismStateChanged ({}): {:?}",
        handlers.len(),
        handlers
    );
    let listeners = actors_handling(&mut game, "OnVampirismStateChanged");
    eprintln!("bound actor references carrying one ({}):", listeners.len());
    for (id, script) in listeners.iter().take(10) {
        let edid = game.world().editor_id(*id).unwrap_or_default();
        eprintln!("  {id} {edid} {script}");
    }

    // Worth recording: the three scripts above are the base classes that
    // declare the event's signature, not subscribers, and no bound actor
    // reference carries a script that handles it. So on a cold start the
    // event has nowhere to land in vanilla data. What this test proves is
    // that the native runs and changes state over vanilla records. That it
    // reaches a listening script is proved by the synthetic scenario in
    // `natives_dlc.rs`, which attaches one. Both halves are needed.
    let mut console = Console::new();
    // The player is the actor the vanilla turn changes, and it is the one
    // reference every install has.
    let before = game.traces().len();
    console
        .run(&mut game, "player.setvampire true")
        .expect("setvampire on the player");
    let player = game.player();
    assert!(
        game.world_mut().actor(player).unwrap().vampire,
        "the player did not come back a vampire"
    );
    eprintln!(
        "player.setvampire true: {} new log lines",
        game.traces().len() - before
    );

    // Cured again, through the same native, so the state is not one-way.
    console
        .run(&mut game, "player.setvampire false")
        .expect("cure the player");
    assert!(!game.world_mut().actor(player).unwrap().vampire);

    // Whatever vanilla actor carries a listening script hears the event.
    if let Some((id, script)) = listeners.first() {
        let edid = game.world().editor_id(*id).unwrap_or_default();
        console
            .run(&mut game, &format!("{edid}.setvampire true"))
            .unwrap_or_else(|e| panic!("setvampire on {edid}: {e}"));
        assert!(
            game.world_mut().actor(*id).unwrap().vampire,
            "{edid} carrying {script} did not change state"
        );
    }
    let errors: Vec<String> = game
        .traces()
        .into_iter()
        .filter(|t| t.contains("no function") || t.contains("no implementation"))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// The lycanthropy sender is the same shape, and the Companions quest line
/// that fires it in play is just as unreachable from a cold start.
#[test]
fn the_werewolf_change_runs_on_the_installed_game() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let handlers = game.vm.scripts_handling("OnLycanthropyStateChanged");
    eprintln!(
        "scripts defining OnLycanthropyStateChanged ({}): {:?}",
        handlers.len(),
        handlers
    );
    let mut console = Console::new();
    console
        .run(&mut game, "player.setwerewolf true")
        .expect("setwerewolf on the player");
    let player = game.player();
    assert!(game.world_mut().actor(player).unwrap().werewolf);
}

/// A quest that is not flagged Start Game Enabled never runs at a cold
/// start. `startquest` and `setstage` drive one on the installed game, which
/// is the shape every quest-flow test wants and the reason `setstage` is the
/// first command anyone reaches for in the vanilla console.
#[test]
fn a_dormant_vanilla_quest_advances_under_debug_commands() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    // A quest the cold start left alone, with stages to walk. Picked from
    // the load order rather than named, so the test does not depend on one
    // quest keeping its flags across game versions.
    let candidates: Vec<GlobalFormId> = game
        .world()
        .order()
        .winners_of(Tag::new(b"QUST"))
        .map(|(id, _)| id)
        .collect();
    let mut chosen = None;
    for id in candidates {
        let dormant = {
            let mut world = game.world_mut();
            match world.quest_state(id) {
                Some(q) => !q.running && !q.completed && q.def.stages.len() >= 5,
                None => false,
            }
        };
        if dormant {
            if let Some(edid) = game.world().editor_id(id) {
                chosen = Some((id, edid));
                break;
            }
        }
    }
    let Some((quest, name)) = chosen else {
        panic!("every quest in the install was already running after start");
    };
    let mut console = Console::new();
    let started = console
        .run(&mut game, &format!("startquest {name}"))
        .unwrap();
    eprintln!("{name} was dormant, startquest said {started}");
    assert_eq!(started, Outcome::Bool(true), "{name} did not start");
    assert!(
        game.world_mut().quest_state(quest).unwrap().running,
        "{name} is not running after startquest"
    );

    // Every stage the record defines, set in order, so the fragments run.
    let stages: Vec<u16> = game
        .world_mut()
        .quest_state(quest)
        .unwrap()
        .def
        .stages
        .iter()
        .map(|s| s.index)
        .collect();
    eprintln!("{name} has {} stages: {stages:?}", stages.len());
    let mut set = 0;
    for stage in &stages {
        if console
            .run(&mut game, &format!("setstage {name} {stage}"))
            .is_ok()
        {
            set += 1;
        }
    }
    let _ = &quest;
    eprintln!("{name}: {set} of {} stages set", stages.len());
    assert!(set > 0, "no stage of {name} could be set");
    let highest = game.world_mut().quest_state(quest).unwrap().highest;
    assert!(highest > 0, "{name} never left stage 0");

    let reported = console.run(&mut game, &format!("getstage {name}")).unwrap();
    assert_eq!(reported, Outcome::Int(i64::from(highest)));
}

/// Package evaluation on a vanilla actor. The package tests are synthetic
/// today because an actor only picks a package once it is loaded and driven,
/// which a cold start does not do for anyone. `prid` then `evp` is the
/// vanilla console's own way to force it, and `evp` is a real console
/// command, so this runs the same sequence a modder would type.
#[test]
fn a_vanilla_actor_evaluates_its_package_under_debug_commands() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    // Hulda runs the Bannered Mare and carries an innkeeper package, the
    // same actor the packages doc already quotes numbers for.
    let name = "Hulda";
    if game.form(name).is_none() {
        eprintln!("{name} is not in this install, skipping");
        return;
    }
    let mut console = Console::new();
    // `Hulda` names the actor base, so the console has to walk to the ACHR
    // that places her. The editor id of a placed reference is usually empty
    // in vanilla data, which is why `pridbase` exists at all.
    let selected = console
        .run(&mut game, &format!("pridbase {name}"))
        .unwrap_or_else(|e| panic!("pridbase {name}: {e}"));
    eprintln!("{name} is placed as {selected}");
    let before = console.run(&mut game, "getcurrentpackage").unwrap();
    let evaluated = console.run(&mut game, "evp").unwrap();
    let after = console.run(&mut game, "getcurrentpackage").unwrap();
    eprintln!("{name}: package before {before}, evp said {evaluated}, after {after}");
    assert_ne!(
        evaluated,
        Outcome::Text("none".into()),
        "{name} resolved no package"
    );
    assert_eq!(evaluated, after, "evp and getcurrentpackage disagree");
}

/// `sv` over a vanilla form, the command a modder uses to see what a script
/// is holding. It is also the check that the VM's variable and property
/// accessors agree with what bind attached.
#[test]
fn script_variables_dump_for_a_vanilla_quest() {
    let Some(mut game) = installed_game() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut console = Console::new();
    // MQ101 is the opening quest, scripted and always present.
    let Some(_) = game.form("MQ101") else {
        eprintln!("MQ101 is not in this install, skipping");
        return;
    };
    let Outcome::List(lines) = console.run(&mut game, "MQ101.sv").unwrap() else {
        panic!("sv did not answer a list");
    };
    eprintln!("MQ101 sv, {} lines:", lines.len());
    for line in lines.iter().take(20) {
        eprintln!("  {line}");
    }
    assert!(
        lines.iter().any(|l| l.starts_with("script ")),
        "no script on MQ101"
    );
}
