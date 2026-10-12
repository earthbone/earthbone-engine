# EDT-11 writer reply

The review identified live selection from an opened load order into the conflict dock as
untested. The Godot editor check now opens the installed five-master Skyrim order, finds a
record with multiple versions, selects its search result through the dock handler, and checks
the dock's plugin order, signatures, Editor IDs and winner markers against the form table.
It reports failure when no conflict is present or when any displayed version differs.

The check is gated by `SKYRIM_DATA` and `SOE_EDT_11_RUNTIME_TEST=1`. It runs in the editor and
quits the editor when complete. This covers the review's missing open-to-selection-to-dock
path. The row's existing Rust test still covers a synthetic three-plugin chain.
