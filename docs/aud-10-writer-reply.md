# AUD-10 writer reply

The installed-data corpus case now requires Rhubarb when Skyrim.esm, the English voice archive and
ffmpeg are available, so a missing recognizer cannot turn the alignment check into a passing skip.
The eight matching INFO responses were processed with and without their plain text hints. Rhubarb
Lip Sync 1.14.0 and ffmpeg were used. Mean absolute track-duration error was 5.4 ms in each group,
below the predeclared 50 ms limit for every track.

The Rhubarb executable was taken from the upstream v1.14.0 Linux release and unpacked under
`/tmp` for the run. The binary is not part of the worktree. The local engine reference corpus did
not contain a Rhubarb checkout, so the upstream release README supplied the `dialogFile` contract:
plain ASCII or UTF-8 text guides recognition, and text that does not match the audio is ignored.

Check command, with `SKYRIM_DATA` and `SOE_RHUBARB` set:

```sh
cargo test -p soe-audio --features rhubarb,xwma-ffmpeg --test visemes -- --nocapture
```

Result: 4 passed in 154.50 seconds, including the eight-line corpus case.
