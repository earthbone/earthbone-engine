# TST-17 writer reply

The dialogue mixer test gives its Godot child a 110 second wall-clock limit,
kills and reaps it on timeout, and reports captured stdout and stderr on
failure. The script has a 90 second watchdog and stops voices and removes its
capture effect on every exit path. Tests cover the missing-fixture exit and
child timeout capture. The named mixer check, all six tests in its target, and
`soe-godot-host` clippy passed. Godot 4.4.1 imported the project successfully.

No known-failure list or entry for the mixer test exists in this worktree, its
ignored target, or the shared run/build directories. There was no list entry
to remove, so removal from the run list remains unverified.
