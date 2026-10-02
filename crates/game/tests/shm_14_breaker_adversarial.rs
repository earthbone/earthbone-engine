// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world_runtime::{FormKey, Projectile, Unsupported, WorldState};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::fs;
use std::process::Command;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_actors() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    for reference in [0x300, 0x100] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), reference)
                .sub(Tag::new(b"NAME"), 0x500_u32.to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(Tag::new(b"MISC"), 0x600));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), 0x200).sub(Tag::new(b"NAME"), 0x600_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Shm14Fixture.esm", plugin).expect("fixture load order");
    Game::new(order)
}

#[test]
fn shm14_process_lists_returns_sorted_built_actors_and_excludes_references() {
    let mut game = game_with_actors();
    let mut world = game.world_mut();
    assert!(world.actor(id(0x300)).is_some());
    assert!(world.actor(id(0x100)).is_some());
    assert!(WorldState::reference_position(&mut *world, FormKey { file: 0, object: 0x200 }).is_some());

    let expected = vec![
        FormKey { file: 0, object: 0x100 },
        FormKey { file: 0, object: 0x300 },
    ];
    assert_eq!(WorldState::loaded_actors(&*world), expected);
    assert_eq!(WorldState::loaded_actors(&*world), expected);
}

#[test]
fn shm14_missing_forms_do_not_resolve_as_world_state_or_runtime_handles() {
    let game = game_with_actors();
    let mut world = game.world_mut();
    let missing = FormKey { file: 0x7fff, object: 0x00ab_cdef };

    assert_eq!(WorldState::reference_position(&mut *world, missing), None);
    assert_eq!(WorldState::actor_value(&mut *world, missing, "health"), None);
    assert_eq!(WorldState::actor_is_dead(&mut *world, missing), None);
    assert_eq!(WorldState::runtime_form_id(&mut *world, missing), None);
}

#[test]
fn shm14_player_and_actor_runtime_ids_round_trip_without_changing_form_keys() {
    let mut game = game_with_actors();
    let mut world = game.world_mut();
    let player = WorldState::player(&mut *world).expect("player form key");
    let player_id = WorldState::runtime_form_id(&mut *world, player).expect("player handle");
    assert_eq!(player, FormKey { file: 0, object: 0x14 });
    assert_eq!(player_id, 0x14);
    assert_eq!(WorldState::runtime_form_id(&mut *world, player), Some(player_id));

    assert!(world.actor(id(0x100)).is_some());
    let actor = FormKey { file: 0, object: 0x100 };
    let runtime_id = WorldState::runtime_form_id(&mut *world, actor).expect("actor handle");
    assert_eq!(world.from_runtime_id(runtime_id), Some(id(0x100)));
}

#[test]
fn shm14_projectile_runtime_is_explicitly_unsupported_on_repeated_calls() {
    let projectile = Projectile {
        handle: soe_commonlib_shim::world_runtime::ProjectileHandle(41),
    };
    assert_eq!(projectile.runtime(), Err(Unsupported::ProjectileRuntime));
    assert_eq!(projectile.runtime(), Err(Unsupported::ProjectileRuntime));
    assert_eq!(projectile.handle.0, 41);
}

#[test]
fn shm14_cpp_zero_runtime_id_does_not_create_a_valid_reference_handle() {
    let scratch = std::env::temp_dir().join(format!("soe-shm14-zero-{}", std::process::id()));
    fs::create_dir_all(&scratch).expect("scratch directory");
    let source = scratch.join("zero.cpp");
    let executable = scratch.join("zero");
    let include = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../commonlib-shim/include");
    fs::write(
        &source,
        r#"
#include <RE/WorldPlayer.h>
bool player(void*, RE::FormKey* out) { *out = {0, 0x14}; return true; }
std::size_t count(void*) { return 0; }
bool actor(void*, std::size_t, RE::FormKey*) { return false; }
bool position(void*, RE::FormKey, RE::Position*) { return false; }
bool value(void*, RE::FormKey, const char*, float*) { return false; }
bool dead(void*, RE::FormKey, bool*) { return false; }
std::uint32_t runtime_id(void*, RE::FormKey) { return 0; }
int main() {
    RE::PlayerWorldStateCallbacks callbacks{nullptr, player, count, actor,
        position, value, dead, runtime_id};
    RE::CallbackPlayerWorldState world(callbacks);
    RE::PlayerCharacter player_character(world);
    return player_character.handle() ? 1 : 0;
}
"#,
    )
    .expect("write C++ handle fixture");
    let compile = Command::new("c++")
        .args(["-std=c++20", "-I"])
        .arg(include)
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("C++20 compiler required for SHM-14");
    assert!(
        compile.status.success(),
        "C++ fixture failed to compile: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable)
        .status()
        .expect("run C++ handle fixture");
    let _ = fs::remove_dir_all(scratch);
    assert!(
        run.success(),
        "zero runtime id must be represented as no ObjectRefHandle"
    );
}
