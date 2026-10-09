# soe-audio: voice and sound

`crates/audio` reads what vanilla ships and decodes what a mod author may ship instead, into one
`Pcm` buffer of interleaved floats. It does not play anything. Playback belongs to the Godot
host through `AudioServer` and `AudioStreamPlayer3D` for positional attenuation. The engine does not link a second device backend.

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
| Switch NX Opus | `opus-decoder` plus our container reader | The same codec dependency reads raw packets. Skyrim-NX-Toolkit and VGAudio define the wrapper and packet layout. | 0 |
| RIFF WAVE | `hound` | Zero transitive dependencies, and the Creation Kit writes WAV. Cheaper than a reader of our own. | 1 |
| WMA v2 bitstream | `oxideav-wma`, in process | Pure Rust WMA v1/v2 decoder, currently matching the vanilla WMA v2 path. | 2 direct crates plus the core crate's transitives |
| FUZ container | ours, following Skyrim-NX-Toolkit | No crate exists. See the credits. | 0 |
| LIP body | ours, zero-run expansion and frame-major weights | Header from `FaceFXWrapper`. Body layout verified against all 74,070 installed English tracks. No dependency added. | 0 |
| xWMA container | ours, ffmpeg's `xwma` demuxer as the reference for `dpds` | No crate exists, and the container is about 120 lines. | 0 |

The audio codec tree includes `lewton`, `byteorder`, `tinyvec`, `ogg`, `opus-decoder`, `hound`,
`oxideav-core` and `oxideav-wma`, plus `thiserror` and `soe-bsa` which the workspace already
carries. `oxideav-core` also brings `bytemuck` and `serde_json` in the default dependency tree.
`cargo tree -p soe-audio` prints the full tree.

### Voice archive locale selection

`[General] sLanguage` selects the voice archive suffix. `VoiceLocale` maps the
seven shipped language names to `en`, `fr`, `de`, `it`, `ja`, `ru` and `es`,
then lets the host retain only matching `* - Voices_<suffix><part>.bsa` entries
from the profile's resolved archive order. It does not order archives or choose
loose files. `soe-bsa::Vfs` does both, with later archives and loose roots
winning.

The local `Skyrim-NX-Toolkit/DefaultIniFiles/Skyrim_fr.ini` and
`Skyrim_de.ini` name `Voices_fr0` and `Voices_de0` respectively. Mutagen's
`GetArchiveIniListingsTests.cs` confirms that the two INI archive lists retain
their written order. A selected archive that is unavailable returns an error.
The host does not substitute English.

### The WMA bitstream

`WmaDecoder` keeps callers independent of a particular implementation. Its `Native`
implementation feeds xWMA block-aligned packets through `oxideav-wma` packet assembly,
frame parsing and synthesis, then returns interleaved float samples. It selects the
short-block exponent-reuse flag for both mono and stereo, as FFmpeg
`libavcodec/wmadec.c` does. The codec factory in version 0.0.4 restricts that flag
to stereo and silently substitutes silence after mono parse failures, so the
adapter uses the lower-level codec API and returns an error on parse failure.
xWMA carries WMA v2 format fields but omits the codec private data found in other
WMA containers. The adapter supplies the setup flags that FFmpeg's xWMA demuxer uses,
normalizes the documented fake bitrate combinations, checks the `dpds` entry count and cumulative
totals, and trims decoder priming output to the exact decoded byte count. It rejects unsupported
tags, invalid packet alignment and decoded PCM above 256 MiB.

`Ffmpeg` remains behind `xwma-ffmpeg` as an out-of-process comparison decoder. It takes the
standalone file produced by `Xwma::to_bytes` on stdin and returns raw float samples. The default
decoder does not start a process or require ffmpeg to be installed.

`oxideav-wma` is MIT licensed and uses a single `oxideav-core` dependency. Its WMA v2 implementation
is a pure Rust codec. Its own WMA wire implementation is under active development, so the gated
vanilla acceptance test compares its output against ffmpeg and checks the output duration and
waveform correlation before claiming support. The local Skyrim-NX-Toolkit conversion script
confirms that xWMA is the PC FUZ audio form and ffmpeg is used for normalization. `fuzmanager`
confirms the FUZ header and payload split, but contains no decoder. Neither local reference
provided a WMA decoder, so the existing Rust package is used rather than porting an unmaintained
or native codec implementation.

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
- **The Switch Opus payload** follows `NXOPUS2FUZ` and `WAV2NXOPUS` in the local
  `Skyrim-NX-Toolkit/Scripts/convert_sound_zappa-demoted.py`. The latter requests
  VGAudio's standard NX header. The inner layout follows
  [`VGAudio's NxOpusReader and NxOpusWriter`](https://github.com/Thealexbarney/VGAudio/tree/9d8f6ea04c83cccccb3dd7851a631bbd53a8dbbe/src/VGAudio/Containers/Opus),
  with encoder delay semantics checked against `Formats/Opus/OpusFormat.cs`.
  VGAudio is MIT licensed. It was absent from the local corpus and fetched for this
  work. Its C# container framework does not fit this Rust crate, so a bounded slice
  reader handles the container and the existing `opus-decoder` handles packets.
- **xWMA**: ffmpeg's `libavformat/xwma.c` is the reference for `dpds` packet totals, fixed packet
  lengths, WMA2 setup bytes and bitrate normalization. Microsoft's `xWMAEncode` writes the format.
- **LIP header**: `../engine-refs/FaceFXWrapper/FFXW32/LipSynchAnim.h` declares
  version, size and flags. `LipSynchAnim.cpp` supplies the compression switch and
  default Skyrim target count. The local checkout now includes its MIT license.
  `../engine-refs/Creation-Kit-Platform-Extended/CKPE.SkyrimSE/Src/Patches/CKPE.SkyrimSE.Patch.LipGen.cpp`
  confirms the external generator path, but does not document body encoding.
- **LIP body**: this implementation comes from byte observations in the user's
  installed archive, with the full acceptance test below. The reviewed
  [OpenFaceFX research codec](https://github.com/OpenFaceFX/OpenFaceFX/blob/7333dc2ffa844e8b3efdecdcfdc813162aea673a/tools/lip_codec_research.py)
  and its exporter at commit `7333dc2` are MIT. They suggested a frame-major
  stream, but cannot serve as a decoder here: they misidentify the flags word
  as a curve count, hardcode compressed offsets and do not expand zero runs
  spanning metadata or float bytes. No code was copied from them. The decoder
  uses the header declaration and corpus-verified zero-run grammar instead.

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

### Switch NX Opus

`SoundSource` decodes a FUZ with the Switch signature to `Pcm` and retains its LIP
bytes. The `opus` feature controls both Ogg Opus and NX Opus and is on by default.
With that feature off, decoding reports `Unsupported::Opus`.

The 20 byte Skyrim wrapper carries the duration in milliseconds, channel count,
header size and payload byte count. Inside it, the standard NX header has type
`0x80000001`, 24 bytes of header fields, version 0, channel count, frame size,
sample rate, data offset and encoder delay. The data chunk has type `0x80000004`
and a byte count. Each packet starts with a big endian byte count and a four byte
encoder range value. Header fields and chunk sizes are little endian.

The decoder accepts mono and stereo at Opus rates of 8, 12, 16, 24 and 48 kHz,
uses the declared rate, and removes encoder delay in frames at that rate. Packet
lengths govern packet boundaries, including variable length packets. The range
value is skipped, as in VGAudio's PCM path. The wrapper's rounded duration does
not supply an exact frame count, so it does not trim the decoded tail. Other NX
header variants are outside Skyrim's standard wrapper contract and rejected.
Decoded PCM is capped at 256 MiB, about 11.6 minutes of 48 kHz stereo, which
bounds voice decoding even when tiny packets expand into long silent frames.
Growth uses checked lengths and fallible allocation within that cap.

AUD-03 acceptance fixtures encode synthetic tones with libopus 1.6 and construct
the NX and FUZ headers in each test. Before the first run, acceptance required
exact frame counts after delay removal, finite nonzero PCM, preserved LIP bytes,
and separate stereo tones in the correct channels. Coverage includes 48 kHz mono,
24 kHz stereo, delay spanning packets, every truncated prefix, invalid offsets,
sizes and header fields, an invalid Opus packet, and a disabled codec feature.
Repeated three byte silence packets test the PCM cap with a small test budget,
including exact capacity at the limit and rejection of the next packet.
No Switch game archive was available for a corpus survey. The PC archive survey
below contains xWMA only and does not establish Switch corpus compatibility.

### xWMA

A RIFF of form `XWMA` with three chunks. `fmt` (with a trailing space) is a `WAVEFORMATEX`. `dpds` is the packet table,
cumulative decoded bytes after each packet, so the last entry over the frame size and the sample
rate gives the exact duration without decoding a byte. `data` is the bitstream. `Xwma::to_bytes`
rebuilds a standalone file, which is what the decoder seam hands over.

### LIP body decoding, AUD-01

`Lip::parse` retains the borrowed header and encoded body. `Lip::decode` returns
`LipWeightTrack`, preserving the signed first frame, target count and every
stored float. `frame_at` uses native integer frame indices. `frames` exposes
mouth targets and the additional gesture channels separately in stored order.

The outer header is twelve bytes, all little endian:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `u32` | Version, 1 |
| 4 | `u32` | Expanded body byte length plus 16 |
| 8 | `u32` | Flags: compressed 1, big endian 2, gestures 4, variable targets 8 |

With compression set, every nonzero byte is a literal. A zero byte introduces
an unsigned little-endian 16-bit count of zero bytes. Runs can span metadata,
float boundaries and entire silent frames. A run count of zero is rejected.
Readable floats in the packed stream do not rule out byte compression. The
2026-09-18 claim in `../research/lip-format.md` that they did was incorrect.

After expansion, the body has a fixed layout:

| Offset | Type | Meaning |
|---|---|---|
| 0 | `u32` | Frame count |
| 4 | `i32` | First native frame index |
| 8 | `u32` | Mouth target count |
| 12 | `f32` array | Frame-major weights, target channels first |

The stride is the target count plus seventeen channels when the gesture flag
is set. The seventeen channels retain their order and values without names.
The expanded body length must equal `12 + 4 * frame_count * stride`, and the
outer size must equal that length plus 16. These formulas held for every
installed English LIP. They are observed layout rules, not a published body
specification. The decoder rejects non-finite weights, truncated runs, trailing
bytes, inconsistent dimensions and expansion beyond 64 MiB. It validates the
stream before allocating the weight array and preserves finite values without
clamping, including negative zero, negative gestures and values above one.

The old negative-marker scan was reading the first frame index. Its varying
position resulted from compressed zero bytes in the frame count. Decoding the
metadata now reads target counts in all 74,070 tracks, including the 3,198
that the marker scan missed. Eight versus sixteen describes mouth targets,
not a loss of eight gesture channels. The gesture flag adds seventeen channels
independently.

`aud_01_known_lip_files_decode_to_authored_weight_tracks` checks a known matrix
against separately hand-written raw and compressed bytes, including pre-roll
and exact float bit patterns. Other fixtures cover gesture values, multi-run
silence, every truncated prefix, oversized dimensions and unsupported flags.
The ordinary gate needs no proprietary asset or generator. The Creation Kit
and `FonixData.cdf` are absent, so these are authored format fixtures, not a
claim that controlled speech was generated with the Creation Kit.

The installed-data acceptance test is
`installed_lip_bodies_decode_every_frame_and_target`, gated by `SKYRIM_DATA`.
It reads the archive through the existing BSA index and streams one FUZ at a
time. It requires all 74,070 nonempty LIP blocks to decode, 73,118 with sixteen
mouth and seventeen additional channels, and 952 with eight mouth channels
and no gestures. It also requires the 1,338 empty LIP payloads to stay empty.
No game asset is copied into the repository.

The 2026-09-20 installed-data run passed in 25.47 seconds: all 74,070 tracks,
9,566,160 frames and 311,857,205 finite samples decoded. The 1,338 empty blocks
were counted separately. The probe also observed gesture weights below zero
and above one, so the decoder preserves their values without clamping.

This decoder supports version 1 little-endian variable-target bodies, with
compressed and uncompressed fixtures. Only compressed files were observed in
the installed corpus. Fixed-target and big-endian variants return a named
unsupported error. Seconds-per-frame, interpolation and named morph binding
are consumer contracts outside this body decoder. The existing `ANM-13` and
`DLG-20` backlog items cover facial binding and playback. A native weight track
is not a Rhubarb `VisemeTrack`, and no weights are converted into approximate
mouth shapes.

## Mouth shapes without the Creation Kit

Audio-derived shapes remain available for lines without a LIP block. Native
LIP weight tracks preserve the animation already authored for existing lines.

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

It sits behind the `VisemeSource` trait and the `rhubarb` feature, off by default, so the default
runtime does not need the binary or its JSON reader. The binary lives in `../tools/rhubarb/` next
to the Godot binary, never committed. Tests find it through `SOE_RHUBARB`, then that directory,
then the path.

Dependency cost, only when the feature is on: five crates, `serde_json`, `serde_core`, `itoa`,
`memchr` and `zmij`, for reading what the tool prints. The default build adds none of them, and
the viseme types and the race mapping work with no tool present.

### INFO response hints, AUD-10

`Rhubarb::visemes_for_response` takes the plain text carried by an ESM `INFO` response and passes
it to Rhubarb through `--dialogFile`, which accepts plain ASCII or UTF-8 text. Rhubarb still
recognizes the recording, while preferring words and phrases that match the hint. It ignores text
where the recording audibly differs. A localized string ID is not resolved in this crate, so the
caller must resolve it before generating cues. Missing or unresolved text uses audio-only
recognition. Rhubarb reports cue times in hundredths of a second.

The gated AUD-10 check selects eight matching Skyrim.esm INFO responses in ascending form ID and
response number from the English voice archive. It generates each track both with and without the
response text and checks that each reported track duration differs from decoded audio duration by
no more than 50 ms. This measures temporal coverage only. Rhubarb exports mouth cues rather than
recognized words, so the check does not claim that hints improve transcript accuracy or exact
phoneme alignment. When the required game files and ffmpeg are available, the check requires
Rhubarb and fails with a tool requirement message if none of the configured locations provides it.

The installed-data run on 2026-10-09 passed for all eight responses with Rhubarb Lip Sync 1.14.0
and ffmpeg. Mean absolute duration error was 5.4 ms for both hinted and unhinted tracks, and each
track stayed within the 50 ms duration bound.

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

### Native and generated tracks

A generated `VisemeTrack` contains discrete mouth cues. A decoded
`LipWeightTrack` contains native float samples. Consumers can use either
source, but binding a native track must preserve its target order and timing
instead of reducing it to the recognizer's shape vocabulary.

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

AUD-02 adds an installed-data check for the default build. It decodes the first English voice
line with `Native`, compares it with ffmpeg's out-of-process output, requires duration agreement
within 60 ms, and requires at least 0.8 waveform correlation. The comparison searches a 4,096
sample priming offset in 32 sample steps and samples every eighth PCM value. It also checks the
native result is non-silent and has the container's sample rate and channel count. This verifies
one vanilla line, not every entry in the archive.

The open path, end to end on one real line: the vanilla xWMA decodes to 2.136 s, the same line
re-encoded to Ogg Vorbis decodes through `lewton` to 2.136 s at 44,100 Hz, and re-encoded to Ogg
Opus decodes through `opus-decoder` to 2.154 s at 48,000 Hz, Opus always resampling to 48 kHz and
its encoder delay being dropped from the header's pre-skip.

## Not done

- Native LIP timing and named morph binding in the playback consumer, tracked by `ANM-13` and `DLG-20`.
- Choosing and running a cache owner. `VisemeCache` writes generated tracks under `.soe-visemes` beside
  a loose asset, or under a caller selected writable cache root for archive content. Its key hashes the
  asset bytes and a canonical settings string. `Rhubarb::visemes_cached` supplies that string from
  the decoded PCM, its program selection, Rhubarb reported version and dialogue text. The byte API
  also lets a caller cache a BSA or other virtual asset without extracting it beside the cache.
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
