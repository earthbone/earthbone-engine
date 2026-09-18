# soe-audio: voice and sound

`crates/audio` reads what vanilla ships and decodes what a mod author may ship instead, into one
`Pcm` buffer of interleaved floats. It does not play anything. Playback belongs to a device crate
in the host, `cpal` or `rodio`, not to a device layer of our own.

The crate follows the engine rule twice over. Vanilla formats are read because a mod can only
express them that way. Ogg Vorbis and Ogg Opus decode beside them because there an author has a
choice, and the open choice should cost nothing.

## Bethesda already did this

The strongest argument for that design is not ours. On the Switch port, Bethesda shipped the same
FUZ container with an **Opus** payload where the PC version carries xWMA. Same twelve byte header,
same LIP block, a different codec after it, chosen per file: `convert_sound_zappa.py` in
Skyrim-NX-Toolkit routes everything under `sound\voice\` to Opus and leaves the rest alone, and
`NXOPUS2FUZ` writes the payload behind the signature `0A 8D D5 FF` followed by the duration in
milliseconds, the channel count, a header size of `0x14` and the payload size.

So "accept an open codec next to the proprietary one under the same descriptors" is not a
liberty this engine is taking. It is a thing the publisher has already shipped in this exact
container, and an engine that reads both is reading real Bethesda data in both cases. `AudioKind`
therefore names the Switch payload rather than lumping it in with the unknowns, even though the
PC archives hold none of it.

## What is ours and what is borrowed

Almost no decoding is written here. The rule was to find who solved it first, take that, and say
so. Where something is ours, it is because nothing packaged existed.

| Format | What we use | Why | Crates added |
|---|---|---|---|
| Ogg Vorbis | `lewton` | Pure Rust, mature, no C toolchain. `symphonia` also decodes Vorbis, but it arrives with a container framework this crate does not need. | 3 (`lewton`, `byteorder`, `tinyvec`) |
| Ogg Opus | `opus-decoder` plus `ogg` | Pure Rust port of libopus. The alternatives (`opus`, `audiopus`, `symphonia-adapter-libopus`) all bind the C library, which this machine would need a toolchain for. Proven here against a real re-encode, see the numbers below. | 2 (`opus-decoder`, `ogg`, sharing `byteorder` with lewton) |
| RIFF WAVE | `hound` | Zero transitive dependencies, and the Creation Kit writes WAV. Cheaper than a reader of our own. | 1 |
| WMA bitstream | ffmpeg, out of process, behind a feature | See below. | 0 |
| FUZ container | ours, following Skyrim-NX-Toolkit | No crate exists. See the credits. | 0 |
| LIP framing | ours, and deliberately incomplete | No crate and no specification exists. | 0 |
| xWMA container | ours, ffmpeg's `xwma` demuxer as the reference for `dpds` | No crate exists, and the container is about 120 lines. | 0 |

Six crates in total, `lewton`, `byteorder`, `tinyvec`, `ogg`, `opus-decoder` and `hound`, plus
`thiserror` and `soe-bsa` which the workspace already carries. `cargo tree -p soe-audio` prints
the lot on one screen.

### The WMA bitstream

ffmpeg has demuxed xWMA and decoded `wmav2` for years, so no WMA decoder is written here and none
should be. What the crate adds is the seam: a `WmaDecoder` trait, and one implementation that runs
the `ffmpeg` binary with the file on its standard input and raw floats coming back.

Out of process, behind a feature that is off by default, is the trade:

- Linking through `ffmpeg-sys-next` needs the development headers on every build machine. This one
  carries `libavcodec.so.62` and no headers at all, so that route does not compile here today.
- Linking drags a large C dependency into a crate the runtime links, and ties the engine's build to
  ffmpeg's ABI across releases.
- A subprocess costs one process per file and nothing else. It is replaceable: the day an
  in-process decoder is worth it, it implements the same trait and no caller changes.
- With no decoder at all, the container reading below still works. A caller gets sample rate,
  channel count, packet count and exact duration, and an `Unsupported::Wma` when it asks for
  samples. That is why the feature can stay off.

## Credits

- **FUZ**: the layout follows `Scripts/convert_sound_zappa.py` in Lord-Akkrand's
  [Skyrim-NX-Toolkit](https://github.com/Lord-Akkrand/Skyrim-NX-Toolkit), which reads the form
  field at offset 8 of the payload and branches on `WAVE` against `XWMA`. That is where the
  payload sniff comes from: the audio half is not always xWMA, so the RIFF form is read rather
  than assumed. [`SockNastre/fuzmanager`](https://github.com/SockNastre/fuzmanager) (C) and
  [`suglasp/pwsh_convert_fuz_to_xwm`](https://github.com/suglasp/pwsh_convert_fuz_to_xwm)
  (PowerShell, no dependencies) are the same container in two more readable forms and agree on
  every field. `fuz_extractor` (Agnahim), Unfuzer and Yakitori Audio Converter read it too but
  ship as binaries. The container is a header and two payloads, so writing those forty lines
  here beats taking a dependency, and all three references agree on what they are.
  Both clones now sit under `../engine-refs/`. `fuzmanager` is GPL-3.0 and this crate is
  GPL-3.0-or-later, so reading it is fine here, but nothing derived from it may reach the MIT shim
  crates. Skyrim-NX-Toolkit is MIT and carries no such restriction.
- **The Switch Opus payload** is recognised but not decoded. `NXOPUS2FUZ` in Skyrim-NX-Toolkit
  gives the header, and [`Thealexbarney/VGAudio`](https://github.com/Thealexbarney/VGAudio)
  implements the NX Opus stream itself. Decoding it would mean reading VGAudio's `NxOpus` reader,
  and it is not needed to run the PC game, so `AudioKind::NxOpus` reports it and stops there.
- **xWMA**: ffmpeg's `libavformat/xwma.c` is the reference for the `dpds` table, and Microsoft's
  `xWMAEncode` is what writes it.
- **LIP**: nothing open decodes it. The tooling survey in
  `../research/open-source-tooling.md` checked every candidate and found that each one copies the
  block as an opaque blob, so this crate does the same rather than spend a branch reverse
  engineering it. `CKPE.SkyrimSE.Patch.LipGen.cpp` in
  `engine-refs/Creation-Kit-Platform-Extended` patches the Creation Kit's lip generation, which
  confirms the track is produced by an external FaceFX tool against `FonixData.cdf` rather than
  by a documented format.

## The containers

### FUZ

Twelve bytes of header, then two payloads.

| Offset | Field |
|---|---|
| 0 | `FUZE` |
| 4 | version, always 1 |
| 8 | LIP block length in bytes |
| 12 | the LIP block, that many bytes |
| 12 + length | the audio, to the end of the file |

`Fuz::split` borrows both halves, `fuz::write` packs them back, and the round trip is exact.
`audio_kind` reads the RIFF form of the audio half rather than assuming.

### xWMA

A RIFF of form `XWMA` with three chunks. `fmt ` is a `WAVEFORMATEX`. `dpds` is the packet table,
cumulative decoded bytes after each packet, so the last entry over the frame size and the sample
rate gives the exact duration without decoding a byte. `data` is the bitstream. `Xwma::to_bytes`
rebuilds a standalone file, which is what the decoder seam hands over.

### LIP, an open gap across the whole ecosystem

This is the one part of the crate that does not do its job, and it is not for want of looking.
The tooling survey checked every open FUZ tool and found the same thing in all of them: none
decodes LIP into phoneme timing, each copies the block through untouched. So this crate splits it
out, frames what is certain, hands the rest back as bytes, and records the gap here rather than
shipping a track that would be invented.

Confirmed over the 74,070 LIP blocks in the English voice archive: the version word is 1 in every
one of them, and header word 2 takes exactly two values, 13 in 73,118 blocks and 9 in 952. Two
values and no others reads like a track or curve count, but that is a histogram, not a decode.

Ruled out by probing the shipped files:

- There is no string table. A FaceFX style named-node serialisation would carry node names, and a
  scan for printable runs finds only float bytes.
- The body is not a fixed stride record list. Walking it as 7-byte records (a `u16` tag, an `f32`
  and a pad) leaves a remainder on most files and produces floats spanning 1e18, which is noise.
- Header word 1 is not a duration. Against real audio lengths from the packet table, the ratio
  ranges from 4,051 to 10,144, so it is not a timebase.
- It is not a simple byte count of the body either, the ratio runs from 3.2 to 4.8.

What works today: a caller can tell whether a line has mouth data, frame the header, carry the
block through the pipeline unchanged, and drive a fallback mouth animation from the audio envelope.
What does not: phoneme weights over time. Cracking that is its own reverse engineering item, and
the honest next step is to generate LIP files from the Creation Kit with known inputs and diff them
rather than to keep staring at shipped bytes.

That gap turned out not to block the feature. See the next section: the mouth shapes are derived
from the audio instead, which covers vanilla lines and mod lines alike, and a decoded LIP block
would feed the same consumer if the format is ever cracked.

## Mouth shapes without the Creation Kit

Decoding LIP was never the goal, a face that moves was. Deriving the shapes from the audio reaches
that goal and reaches further than vanilla does, so this is the route the crate takes.

The case for it is not convenience. A mod that adds voice lines has to run them through the
Creation Kit and its `Data/Sound/Voice/Processing/FonixData.cdf` speech database to get a LIP
block, and that path is broken often enough on Special Edition that the community ships a mod
purely to repair the database file. Generating from audio removes the editor from the loop: a line
with no LIP block animates exactly like a vanilla one. Nothing in `viseme.rs` treats a vanilla line
as a special case, which is what makes that true rather than aspirational.

### The tool

Rhubarb Lip Sync (MIT, `DanielSWolf/rhubarb-lip-sync`, v1.14.0, Linux build published) listens to a
recording and reports which of nine mouth positions to hold at each moment, optionally constrained
by the line's written text. Its licence notes that the data it produces belongs to whoever ran it,
so a generated track carries no encumbrance at all.

It sits behind the `VisemeSource` trait and the `rhubarb` feature, off by default, exactly where
ffmpeg sits for the WMA bitstream and for the same reasons. The binary lives in `../tools/rhubarb/`
next to the Godot binary, never committed. Tests find it through `SOE_RHUBARB`, then that
directory, then the path.

Dependency cost, only when the feature is on: five crates, `serde_json`, `serde_core`, `itoa`,
`memchr` and `zmij`, for reading what the tool prints. The default build adds none of them, and
the viseme types and the race mapping work with no tool present.

### Shapes onto phonemes

Rhubarb emits mouth positions and a race maps morph targets per phoneme, so one sound from each
shape stands for its group. Every choice is taken from Rhubarb's own description of the shape in
its `README.adoc`, and every name below appears in the `PHWT` phoneme list.

| Shape | Rhubarb's description | Phoneme |
|---|---|---|
| A | closed mouth, P B M | `M` |
| B | clenched teeth, most consonants and EE | `K` |
| C | open, EH as in men, AE as in bat | `EH` |
| D | wide open, AA as in father | `AA` |
| E | slightly rounded, AO as in off, ER as in bird | `AO` |
| F | puckered, UW as in you, OW as in show, W as in way | `UW` |
| G | upper teeth on lower lip, F and V | `F` |
| H | tongue raised, long L | `L` |
| X | rest position | `SIL` |

The weights themselves stay on the `RACE` record, which is what makes a dragon's face differ from a
Nord's with none of that knowledge in this crate.

### The race mapping is sparse, which was a surprise

`RACE` carries the mapping in two subrecords: `PHTN` names the morph targets and `PHWT` gives, for
each of the 43 FaceFX phonemes in a fixed order, one weight per target. `crates/esm` reads both.

Over Skyrim.esm, only 6 of 99 races carry a complete mapping:

| Races | Targets |
|---|---|
| `DragonRace`, `AlduinRace`, `UndeadDragonRace`, `HagravenRace` | 8, the `Lip` prefixed set |
| `WerewolfBeastRace`, `dunMiddenEmptyRace` | 16, the humanoid set |

Every playable race carries none, `NordRace` included. So reading only the actor's own race leaves
almost every face in the game still, and a consumer needs a fallback. The humanoid table does exist
in the file, just not on the races that need it, which is why `mapping_race(actor, donor)` resolves
through a donor rather than inventing weights. Which donor is the caller's policy, not this
crate's. A gated test pins both halves of this finding, so if a future file changes it the test
says so.

### Measured, and what the numbers mean

Over 8 real voice lines from the English archive, decoded through ffmpeg and run through Rhubarb:

| Measurement | Value |
|---|---|
| Tracks produced | 8 of 8 |
| Audio covered | 16.30 s |
| Wall clock | 19.67 s, 2.46 s per line |
| Speed | 0.83 times realtime |

Slower than realtime is the number that decides how this is used. Generating shapes is an asset
step, not a playback step: a track is derived once and cached beside the line, the same way the
Creation Kit writes a LIP block once. Doing the whole English archive this way is hours of work,
not milliseconds, and the tool takes a `--threads` option for it.

Shape distribution over those lines, 38 cues: `X` 42.1 percent, `B` 26.3, `C` 10.5, `D` 7.9,
`F` 7.9, `E` 5.3. Rest dominating is what speech with pauses looks like.

The case vanilla cannot do, as a gated test: a real line is re-encoded to Ogg Vorbis, which drops
the FUZ container and its LIP block, decoded through `lewton`, and run through the same pipeline. A
2.32 second mod line produced 5 cues and moved a real race's morph targets across 140 sampled
frames. No Creation Kit, no `FonixData.cdf`, no LIP block anywhere in the chain.

### Why this is not wasted if LIP is cracked

A generated track and a decoded LIP block are the same `VisemeTrack`, and `morph_weights` never
learns which it got. Cracking the format later adds a second `VisemeSource` and changes nothing
downstream.

## Measured against the installed game

`Skyrim - Voices_en0.bsa`, 2026-09-18, gated on `SKYRIM_DATA`:

| Measurement | Value |
|---|---|
| Voice files | 75,408, every one splits |
| With a LIP block | 74,070, empty 1,338 |
| Audio half | 75,408 xWMA, 0 WAVE, 0 unrecognised |
| Format tag | `0x0161` (`WMAUDIO2`) on every file |
| Sample rate | 44,100 Hz on every file |
| Channels | mono 74,869, stereo 539 |
| Packets | 957,204 |
| Total audio | 283,699 s, about 78.8 hours |
| Time to split and read every container | 0.12 s |

Decoding, with `--features xwma-ffmpeg`: eight lines decoded, every one within 60 ms of the
duration its packet table predicts, which is the cross-check that the container reading and the
decode agree.

The open path, end to end on one real line: the vanilla xWMA decodes to 2.136 s, the same line
re-encoded to Ogg Vorbis decodes through `lewton` to 2.136 s at 44,100 Hz, and re-encoded to Ogg
Opus decodes through `opus-decoder` to 2.154 s at 48,000 Hz, Opus always resampling to 48 kHz and
its encoder delay being dropped from the header's pre-skip.

## Not done

- Phoneme decoding, above. The mouth moves without it.
- Caching a generated track beside its line. At 0.83 times realtime this has to be an asset step
  with a cache, and nothing here writes one yet.
- Choosing the donor race for an actor whose own race carries no mapping. `mapping_race` takes the
  donor as an argument on purpose, and picking it belongs to whatever drives a face.
- Feeding the line's written text to the recogniser. `VisemeSource::visemes` takes it and the
  Rhubarb implementation passes it through, but nothing resolves a line's `INFO` text yet, so every
  measurement above ran without it and would sharpen with it.
- `SNDR`, `SOUN`, `SNCT` and `SOPM` record views. `SoundSource` resolves a data path through the
  load order and tries the `fuz`, `xwm` and `wav` extensions the way the game does, which is the
  useful half, but a descriptor form id does not resolve to a path yet. That wants the record views
  in `crates/esm` and a later pass.
- Music and ambience, which are the same containers through a different descriptor.
- Playback. No device, no mixer, no 3D panning.
