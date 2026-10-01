// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn invalid_actor_reads_remain_explicit_at_the_cpp_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let rust = fs::read_to_string(root.join("src/world.rs")).unwrap();
    let cpp = fs::read_to_string(root.join("include/RE/World.h")).unwrap();
    assert!(rust.contains("Result<ActorCriticalStage, UnsupportedState>"));
    assert!(
        cpp.contains("Result<ACTOR_CRITICAL_STAGE"),
        "C++ critical_stage returns a defaultable enum for missing actors"
    );
}
