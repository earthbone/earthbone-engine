// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-37 scheduling contracts through public Game and simulation APIs.

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use soe_esm::LoadOrder;
use soe_game::simulation::{Command, Config, Simulation, SnapshotPair};
use soe_game::Game;
use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::vm::NativeReturn;
use soe_papyrus::{Opcode, Value};

fn wait_for(
    simulation: &Simulation,
    predicate: impl Fn(&SnapshotPair) -> bool,
) -> Arc<SnapshotPair> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(simulation.failure().is_none(), "simulation worker failed");
        if let Some(pair) = simulation.poll().filter(|pair| predicate(pair)) {
            return pair;
        }
        assert!(
            Instant::now() < deadline,
            "simulation did not publish the requested state"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn slow_script(
    game: &mut Game,
    calls: Arc<std::sync::atomic::AtomicUsize>,
) -> soe_papyrus::ThreadId {
    game.vm.natives.register_global(
        "BudgetCheck",
        "Slow",
        Box::new(move |_| {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            NativeReturn::Value(Value::None)
        }),
    );
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .local("unused", "None");
    for _ in 0..3 {
        function = function.ins(
            Opcode::CallStatic,
            vec![s("BudgetCheck"), s("Slow"), ident("unused")],
        );
    }
    let script = ScriptBuilder::new("BudgetCheckScript", "")
        .function(function.ins(Opcode::Return, vec![none()]).build())
        .bytes();
    game.load_script(&script).unwrap();
    game.vm
        .call_global("BudgetCheckScript", "Run", &[])
        .unwrap()
}

#[test]
fn prf_37_papyrus_deadline_is_shared_and_retains_runnable_stacks() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut game = Game::new(LoadOrder::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let thread = slow_script(&mut game, Arc::clone(&calls));
    game.with_papyrus_budget(Duration::from_micros(1200), |game| {
        assert!(!game.run_until_idle(64));
        assert!(!game.with_papyrus_budget(Duration::from_secs(1), |game| game.run_until_idle(64)));
    });
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "one atomic native overrun must not renew the budget"
    );
    assert!(game.vm.thread_result(thread).is_none());
    assert!(game.run_until_idle(64));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn prf_37_zero_script_budget_defers_execution_but_advances_vm_clocks() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut game = Game::new(LoadOrder::new());
    let calls = Arc::new(AtomicUsize::new(0));
    slow_script(&mut game, Arc::clone(&calls));
    game.tick_scheduled(0.5, false, None, Duration::ZERO);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(game.vm.time(), 0.5);
    assert_eq!(game.world().clock.real_seconds, 0.5);
}

#[test]
fn prf_37_other_simulation_work_does_not_spend_the_papyrus_allowance() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut game = Game::new(LoadOrder::new());
    let calls = Arc::new(AtomicUsize::new(0));
    slow_script(&mut game, Arc::clone(&calls));
    game.with_papyrus_budget(Duration::from_micros(1200), |game| {
        std::thread::sleep(Duration::from_millis(5));
        game.run_until_idle(64);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn prf_37_simulation_is_fixed_rate_independent_of_render_polling() {
    let simulation =
        Simulation::spawn(Config::default(), || Ok(Game::new(LoadOrder::new()))).unwrap();
    let first = wait_for(&simulation, |pair| pair.current.sequence >= 2);
    let started = Instant::now();
    for _ in 0..10_000 {
        std::hint::black_box(simulation.poll());
    }
    std::thread::sleep(Duration::from_millis(80));
    let latest = wait_for(&simulation, |pair| {
        pair.current.sequence > first.current.sequence
    });
    let elapsed = started.elapsed();
    assert!(
        latest.current.sequence - first.current.sequence
            <= (elapsed.as_secs_f64() * 60.0).ceil() as u64 + 1
    );
    assert_eq!(latest.previous.sequence + 1, latest.current.sequence);
    assert!((latest.current.seconds - latest.current.sequence as f64 / 60.0).abs() < 1e-9);
}

#[test]
fn prf_37_game_initialization_and_destruction_stay_on_the_simulation_thread() {
    struct Owner(mpsc::Sender<std::thread::ThreadId>);
    impl Owner {
        fn touch(&self) {}
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            self.0.send(std::thread::current().id()).unwrap();
        }
    }
    let caller = std::thread::current().id();
    let (sent, received) = mpsc::channel();
    let simulation = Simulation::spawn(Config::default(), move || {
        sent.send(std::thread::current().id()).unwrap();
        let owner = Owner(sent);
        let mut game = Game::new(LoadOrder::new());
        game.vm.natives.register_global(
            "Owner",
            "Hold",
            Box::new(move |_| {
                owner.touch();
                NativeReturn::Value(Value::None)
            }),
        );
        Ok(game)
    })
    .unwrap();
    let constructed = received.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_for(&simulation, |pair| pair.current.sequence >= 1);
    drop(simulation);
    assert_ne!(constructed, caller);
    assert_eq!(
        received.recv_timeout(Duration::from_secs(5)).unwrap(),
        constructed
    );
}

#[test]
fn prf_37_pause_commands_keep_order_and_do_not_accumulate_world_time() {
    let simulation =
        Simulation::spawn(Config::default(), || Ok(Game::new(LoadOrder::new()))).unwrap();
    let first = wait_for(&simulation, |pair| pair.current.sequence >= 2);
    assert!(simulation.command(Command::MenuMode(true)));
    let paused = wait_for(&simulation, |pair| {
        pair.current.sequence >= first.current.sequence + 3
    });
    let later = wait_for(&simulation, |pair| {
        pair.current.sequence >= paused.current.sequence + 3
    });
    assert_eq!(paused.current.seconds, later.current.seconds);
    assert!(simulation.command(Command::MenuMode(false)));
    let resumed = wait_for(&simulation, |pair| {
        pair.current.seconds > later.current.seconds
    });
    assert!((resumed.current.seconds - later.current.seconds - 1.0 / 60.0).abs() < 1e-9);
}

#[test]
fn prf_37_worker_initialization_errors_are_observable_without_waiting() {
    let simulation =
        Simulation::spawn(Config::default(), || Err("invalid load order".into())).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(error) = simulation.failure() {
            assert_eq!(error, "invalid load order");
            break;
        }
        assert!(simulation.poll().is_none());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn prf_37_vanilla_bannered_mare_publishes_form_backed_actor_snapshots() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let simulation = Simulation::spawn(Config::default(), move || {
        let mut game = Game::from_shared_order(order);
        let cell = game
            .world_mut()
            .cell_by_editor_id("WhiterunBanneredMare")
            .unwrap();
        game.load_cell(cell);
        let references = game.world_mut().cell_references(cell);
        for id in references {
            if game
                .world()
                .record(id)
                .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
            {
                game.world_mut().actor(id);
            }
        }
        Ok(game)
    })
    .unwrap();
    let pair = wait_for(&simulation, |pair| pair.current.evaluations >= 5);
    assert!(pair.current.actors.len() >= 5);
    assert!(pair
        .current
        .actors
        .values()
        .all(|pose| pose.position.into_iter().all(f32::is_finite)));
}

#[test]
fn prf_37_vanilla_persistent_plains_actors_require_attached_grid() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let expected_order = Arc::clone(&order);
    let simulation = Simulation::spawn(Config::default(), move || {
        let mut game = Game::from_shared_order(Arc::clone(&order));
        let world = game.world_mut().form("Tamriel").unwrap();
        let index = order.build_cell_index();
        let focus = index.exterior_cell(world, 4, -2).unwrap();
        let persistent = index.world_persistent_cell(world).unwrap();
        let mut resident = (3..=5)
            .flat_map(|x| (-3..=-1).filter_map(move |y| index.exterior_cell(world, x, y)))
            .filter(|id| *id != focus)
            .collect::<Vec<_>>();
        resident.push(persistent);
        resident.sort_unstable();
        resident.dedup();
        game.change_cell(Some(focus));
        game.attach_streaming_cells(&resident).unwrap();
        Ok(game)
    })
    .unwrap();
    let pair = wait_for(&simulation, |pair| !pair.current.contacts.is_empty());
    assert!(
        pair.current.contacts.iter().any(|actor| {
            expected_order
                .build_cell_index()
                .grid_of(actor.cell)
                .is_none()
        }),
        "worldspace persistent actors are present in the resident snapshot"
    );
    for actor in pair.current.contacts.iter() {
        assert!((3..=5).contains(&((actor.position[0] / 4096.0).floor() as i32)));
        assert!((-3..=-1).contains(&((actor.position[1] / 4096.0).floor() as i32)));
    }
}
