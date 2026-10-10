# SCR-11 writer reply

The native bindings, actor section and determinism coverage were present in
the starting branch. This change connects the saved vehicle and forced-look
relationships to shared actor placement during the AI tick. Vehicle movement
preserves the actor's relative offset and orientation. Look-at changes actor
yaw, and pathing requests use the AI navigation step to approach the live
target. Save loading rebuilds a clean transient vehicle motion baseline. The
host observes placement changes through the existing reference path.

The local CK pages read were `SetVehicle_-_Actor.wikitext`,
`GetSitState_-_Actor.wikitext` and `GetFlyingState_-_Actor.wikitext`. Their
contracts are checked alongside synthetic argument and state tests. No
installed-data test is required by SCR-11's check because it names no vanilla
records.

Checks run: `cargo fmt --all`, 15 tests passed with
`cargo test -p soe-game --test it scr_11_`, and
`cargo clippy -p soe-game --all-targets -- -D warnings` passed.
