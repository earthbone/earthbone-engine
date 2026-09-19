# Parser fuzzing

Every format crate parses bytes a mod author, not the engine, controls: a
plugin, an archive, a mesh, a behavior graph, a texture, a compiled script.
None of that input is trusted. `fuzz/` is a `cargo-fuzz` harness that feeds
random and mutated bytes to each parser's entry point and watches for panics,
aborts and out-of-memory kills. It is a separate Cargo workspace (its own
`[workspace]` table in `fuzz/Cargo.toml`) so `cargo test --workspace` at the
root never needs the nightly toolchain cargo-fuzz requires.

## How to run

Install once: `cargo install cargo-fuzz` on a nightly toolchain (`rustup
toolchain install nightly`).

If `rustup` and a Homebrew or system Rust are both on `PATH`, the bare
`cargo +nightly fuzz` invocation can resolve to the wrong `cargo`, and even
when it resolves correctly, cargo-fuzz's own internal `rustc` subprocess
calls can still pick up a stable `rustc` ahead of rustup's proxy. Put
rustup's bin directory first for the whole command:

```sh
PATH="$HOME/.cargo/bin:$PATH" "$HOME/.cargo/bin/cargo" +nightly fuzz run \
  --fuzz-dir fuzz <target> -- -max_total_time=600
```

List targets with `cargo +nightly fuzz list --fuzz-dir fuzz`. Reproduce a
saved crash by passing its artifact path as a positional argument instead of
`-max_total_time`:

```sh
PATH="$HOME/.cargo/bin:$PATH" "$HOME/.cargo/bin/cargo" +nightly fuzz run \
  --fuzz-dir fuzz <target> fuzz/artifacts/<target>/<artifact-id>
```

## Targets and entry points

| Target | Crate | Entry point |
|---|---|---|
| `fuzz-esm` | `soe-esm` | `Plugin::parse` |
| `fuzz-bsa` | `soe-bsa` | `index_archive_in` |
| `fuzz-nif` | `soe-nif` | `parse` |
| `fuzz-hkx` | `soe-hkx` | `Packfile::parse` |
| `fuzz-dds` | `soe-dds` | `Dds::parse` |
| `fuzz-pex` | `soe-papyrus` | `pex::parse` |
| `fuzz-pex-grammar` | `soe-papyrus` | Generated PEX round trips |
| `fuzz-vm-run` | `soe-papyrus` | Generated bounded VM execution |
| `fuzz-save` | `soe-save` | Save parse and decompressed-body round trip |
| `fuzz-dds-grammar` | `soe-dds` | Generated DDS decode |
| `fuzz-bsa-grammar` | `soe-bsa` | Generated archive round trips |
| `fuzz-audio` | `soe-audio` | FUZ, LIP, RIFF and xWMA containers |
| `fuzz-profile` | `soe-profile` | INI, plugins.txt and loadorder.txt |
| `fuzz-moddata` | `soe-moddata` | SPID, KID and Base Object Swapper text |
| `fuzz-vm-state` | `soe-papyrus` | SOEV load followed by bounded execution |

Targets in `fuzz/fuzz_targets/` exercise parser error paths, round trips and
bounded execution. The save target compares the uncompressed header and body,
not compressor output, which can differ without losing data. The VM-state
target preloads a probe script, allowing mutated valid frames to execute.
Profile round trips cover named INI sections and representable plugin names,
while arbitrary malformed text still exercises the parsers.

## Seeds and corpus

`fuzz/src/bin/gen_seeds.rs` writes minimal valid fixtures for the six original
byte targets and the four new targets into
`fuzz/corpus/<target>/`, built with the same crate the target under test
uses (the DDS seed comes from `Header::to_bytes`, the PEX seed from
`soe_papyrus::asm::ScriptBuilder`, and so on), so the fuzzer starts from a
file libFuzzer's mutator can meaningfully evolve rather than raw noise.
`fuzz/src/bin/check_seeds.rs` asserts every seed still parses `Ok` through
its target's entry point, as a fast sanity check independent of the fuzzer
itself. Run either with `cargo run --manifest-path fuzz/Cargo.toml --bin
gen_seeds` or `check_seeds`.

The checker exits with failure for rejected binary seeds. The two permissive
text parsers cannot reject input, so their seeds are checked as nonempty UTF-8.
Grammar targets generate their own structure. The save target uses the external
save corpus, which must never be copied into this repository.

`fuzz/.gitignore` excludes `corpus`, `artifacts`, `target` and `coverage`, so
none of that is committed. A libFuzzer session can grow its corpus into
thousands of files within minutes, and none of it is needed to reproduce a
result: the seeds are regenerated on demand by `gen_seeds`, and every crash
this round found is captured for good as a regression test with the
crash-triggering bytes embedded directly in the crate's test module (see
below), not as a file under `fuzz/artifacts/`. Run `gen_seeds` after cloning,
before the first fuzz run.

## Results

Ten minutes (`-max_total_time=560`, wall clock 561 seconds once libFuzzer's
own setup and teardown are counted) per target, run one at a time, using the
corpus above. Executions per second and total runs are read from each
target's final libFuzzer status line.

| Target | Runs | Exec/s | Crashes found |
|---|---|---|---|
| `fuzz-esm` | 1,346,481 | 2,400 | 0 this run (1 fixed previously, see below) |
| `fuzz-bsa` | 29,544,753 | 52,664 | 0 |
| `fuzz-nif` | 59,942,027 | 106,848 | 0 |
| `fuzz-hkx` | 81,805,174 | 145,820 | 0 |
| `fuzz-dds` | 1,320,298 | 2,353 | 0 this run (4 fixed previously, see below) |
| `fuzz-pex` | 9,617,780 | 17,143 | 0 |

`fuzz-esm` and `fuzz-dds` run far fewer executions per second than the other
four because their parsers do real work per input (record decompression for
esm, per-mip-level bookkeeping for dds) instead of failing fast on a
malformed header. `fuzz-bsa`'s run overlapped briefly with a second
invocation of the same target during this session, which shows up as an
interleaved status block partway through its log, but the final tally above
is from a single continuous process and is not affected.

## Crashes found and fixed

Five inputs found real bugs, all before this session's clean run. Each fix
clamps an attacker-controlled count read from a file header to a bound no
real Skyrim SE asset comes close to, before that count drives allocation or
a bit shift.

- `crates/dds/src/format.rs`, `Masks::validate`: a bit count field could
  reach 32 or more and overflow a `1u32 << bits` shift. Fixed with
  `checked_shl`.
- `crates/dds/src/format.rs`, `Format::slice_bytes`: width and height near
  `u32::MAX` overflowed the byte size multiplication. Fixed with
  `saturating_mul`.
- `crates/dds/src/lib.rs` and `header.rs`, `Header::mip_count`: a huge
  declared mip count shifted a dimension left by more than 31 bits. Fixed by
  clamping the count to 32 before it is used.
- `crates/dds/src/header.rs`, `Header::layer_count`
  (`fuzz/artifacts/fuzz-dds/oom-b95d4d129f6dc8f49e040a4eb03b2a19025aee08`,
  found and fixed this session): the DX10 extension's `array_size` could sit
  near `u32::MAX`, and `Dds::parse` pushed one `Level` per layer per mip
  before any per-level size check ran, so the level `Vec` grew until the
  process was killed for exhausting memory. Fixed by clamping `array_size`
  to `header::MAX_ARRAY_SIZE` (4096) before it multiplies into the layer
  count.
- `crates/esm/src/plugin.rs`: a compressed record's declared expected size
  preallocated an inflate buffer of that size directly, so a bogus header
  field asked for a multi-gigabyte allocation. Fixed by capping the
  preallocation at `MAX_INFLATE_PREALLOC` (64 MiB) and letting the buffer
  grow normally past that if the record is genuinely large.

Every fix has a regression test in its crate that replays the exact
crash-triggering bytes, so a future refactor that reopens one of these paths
fails `cargo test` immediately rather than waiting for the fuzzer to find it
again. `fuzz/artifacts/` itself is gitignored and local only, so the embedded
bytes in each regression test are the lasting record of the finds, not the
artifact files.

## Audit harness verification (2026-09-19)

All targets build on stable Rust with the locked dependencies. The four new
targets each completed 10,000 bounded mutation inputs, after correcting an
overstrict INI assertion about empty unnamed sections. The seed generator and
checker passed. The save target also passed all 60 external saves as fixed
inputs, preserving their headers and decompressed bodies without copying them.

These smoke runs have no coverage instrumentation or sanitizer support. They
are not replacements for the nightly cargo-fuzz runs described above. No
nightly toolchain or cargo-fuzz executable was installed on the test machine.
