# Rust coding style

The baseline for this workspace is the Apollo GraphQL team's
[Rust Best Practices](https://github.com/apollographql/rust-best-practices) handbook,
condensed below to the parts that apply to a workspace of parser and runtime library
crates (`esm`, `bsa`, `nif`, `papyrus`, `hkx`, `dds`, `physics`, `navmesh`, `profile`,
`commonlib-shim`, `godot-host`, `game`) with no async runtime and no web surface. The
"house rules" section adds constraints specific to reverse-engineered file formats and
a Godot-hosted runtime.

## Rules we hold

### Errors and panics

- Return `Result` from any function that can fail.
- Never call `.unwrap()` or `.expect()` outside tests and `#[cfg(test)]` fixtures.
- Reserve `panic!`, `unreachable!`, `todo!`, and `unimplemented!` for bugs and
  unfinished code, never for malformed input.
- Derive crate errors with `thiserror`, not a hand-written `Error` impl.
- Use one error struct when a module has a single failure kind, an enum when it has
  several, with `#[from]` conversions for wrapped causes.
- Use `?` to bubble an error, `map_err` or `inspect_err` to transform or log one.
- Keep `anyhow` out of library crates, it erases the context a caller needs.
- Avoid `Box<dyn std::error::Error>` in a public signature unless the crate genuinely
  needs an open error set.
- Write a unit test for the error path of a function, not only its happy path.

### Ownership and borrowing

- Default to borrowing: take `&T`, `&[T]`, or `&str` instead of an owned parameter.
- Ask for ownership in the signature when a function truly needs it, do not clone a
  borrowed argument internally to fake it.
- Clone `Arc` and `Rc` handles freely, avoid cloning the contents of a `Vec` or
  `HashMap`.
- Derive `Copy` only on small plain-data types, roughly 2 to 3 machine words, never on
  a type that also implements `Iterator`.
- Prefer a `for` loop for early exit or side effects, an iterator chain for a
  transform.
- Use `unwrap_or_default` and the `_else` fallback variants instead of a hand-rolled
  default branch.
- Pass or return large or recursive structures behind `Box` rather than by value.

### API shape

- Keep items private by default, export only what a caller needs.
- Wrap a raw offset, hash, or index in a newtype instead of passing a bare integer.
- Favor static dispatch, generics or `impl Trait`, over `dyn Trait` unless runtime
  polymorphism is required.
- Put `dyn Trait` behind a pointer, `Box`, `Arc`, or `&`, never bare, and box it at the
  API boundary rather than inside a struct.
- Keep a trait object-safe when it may be used as a trait object: no generic methods,
  no `Self: Sized`, every method takes `self` in some borrowed or owned form.
- Use the type-state pattern for a struct with a real invalid-use-before-init hazard
  (an unopened archive, an unparsed record), not for trivial state.
- Prefer a builder with `fn with_x(&mut self, ...) -> &mut Self` over one that takes
  and returns owned `self`, unless owned mutation is required.

### Naming

- Extract a function to name a concept, not to deduplicate two similar lines, wait for
  a genuine third occurrence of the same decision.
- Do not add a boolean flag parameter to fake reuse, use a two-variant enum if a mode
  selector is unavoidable.
- Name a test as unit of work, expected behavior, and state, never a generic name like
  `test_it_works`.
- Group a unit's tests under one `#[cfg(test)] mod` named after that unit.

### Tests

- Put unit tests in the same module as the code, under `#[cfg(test)]`.
- Put integration tests under `tests/`, exercising only the public API.
- Keep one behavior, and close to one assertion, per test.
- Use `assert_eq!` for equality and `assert!(matches!(..))` for a shape check, both
  with a failure message.
- Assert an error path with `.unwrap_err()`, compared by `PartialEq` when the error
  implements it, otherwise by `to_string()`.
- Use `cargo insta` snapshots for structural or generated output, not for a small or
  numeric value.
- Share setup functions or `rstest` fixtures across tests, never share the test logic
  itself.

### Comments and docs

- Use `//` only for the why, a safety argument, or a non-obvious tradeoff, never to
  restate the code.
- Tag a reasoning comment, for example `SAFETY:` or `CONTEXT:`, so its role is clear at
  a glance.
- Fix or delete a comment the moment it stops matching the code beside it.
- Turn a `TODO` into a tracked issue and reference it, `TODO(issue #N): ...`, never
  leave one ownerless.
- Document every public function, struct, trait, and enum with `///`, covering
  purpose, `# Errors`, and `# Panics`.
- Put a `//!` module doc at the top of `lib.rs` and every `mod.rs` stating what the
  module is for.
- Keep runnable examples inside doc comments so `cargo test` exercises them.

### Dependencies

- Reserve `anyhow` for binaries and test helpers, use `thiserror` for library error
  types.
- Run `cargo clippy --all-targets --all-features --locked` before calling a change
  done, fix `Cargo.lock` drift with `cargo update` rather than dropping `--locked`.
- Prefer a small, well-scoped crate for a narrow need, for example `smallvec` for a
  large const array, over a general-purpose dependency.

## House rules on top

- Comments are minimal. Say the why once, in the fewest words that carry it.
- Every module's `//!` doc states the format or design it implements and names its
  source: a published spec, reverse-engineering notes, or an upstream reference
  implementation.
- Inline comments are rare, used only where the code itself cannot say the thing.
- No `unsafe` outside the `godot-host` crate.
- Malformed input returns a typed error. It never panics.
- Tests that read real game files are gated behind the `SKYRIM_DATA` environment
  variable and skip cleanly when it is unset.
- Every other test runs against a synthetic fixture the test itself writes.

## Enforced mechanically

| Rule | Tool |
|---|---|
| Formatting, import grouping and ordering | `cargo fmt` |
| No `.unwrap()` outside tests | `clippy::unwrap_used` |
| No `.expect()` outside tests | `clippy::expect_used` |
| No panics outside tests and truly unreachable branches | `clippy::panic` |
| No panicking raw indexing | `clippy::indexing_slicing` (not enabled, too noisy for parser code that already bounds-checks by hand) |
| Public functions document their `# Errors` section | `clippy::missing_errors_doc` (currently allowed at workspace level, see "Running the gate") |
| Public functions document their `# Panics` section | `clippy::missing_panics_doc` (currently allowed at workspace level, see "Running the gate") |
| Pure value-returning functions are marked `#[must_use]` | `clippy::must_use_candidate` (currently allowed at workspace level, see "Running the gate") |
| Broader style, complexity, and correctness lints | `clippy::pedantic` group |
| No `unsafe` outside `godot-host` | `unsafe_code` rustc lint, denied at workspace level, allowed only in the `godot-host` crate |
| Comment prose meets house style, no filler, no stale `TODO`, tagged reasoning comments | `comment-lint` |
| Module doc states format, design, and source | review |
| Newtype, builder, and trait-object API shape choices | review |
| Test naming and one-behavior-per-test | review |
| Real-file tests gated on `SKYRIM_DATA`, all other fixtures synthetic | review |
| `anyhow` kept out of library crates, dependency scope kept narrow | review |

## Running the gate

`tools/lint.sh` runs every mechanical check in the order above and stops at the first
failure. Run it before committing:

```bash
tools/lint.sh
```

The gate runs `cargo deny check` and `cargo audit` against the workspace dependency
tree. The GitHub workflow installs pinned versions of both tools. `deny.toml` defines
the accepted dependency licenses and records the advisory exception for the locked
`paste` dependency.

Wire it in as a pre-commit hook once, per clone:

```bash
git config core.hooksPath tools
```

`tools/pre-commit` just calls `tools/lint.sh`, so this makes the gate run automatically
on every `git commit`. It only affects your own working copy, nothing to undo if you
remove the hook path later (`git config --unset core.hooksPath`).

### How the workspace lint allow list was measured

`[workspace.lints.clippy]` in the root `Cargo.toml` allows a set of `pedantic` lints
that fire noisily across the reverse-engineering crates (raw field widths need casts,
binary formats have magic numbers, and so on). The allow list is not a guess: it comes
from running clippy in JSON mode and counting real hits, scoped to `--lib --bins` so
`unwrap_used`/`expect_used`/`panic` are measured on production code only, since tests
are separately exempted by the `#![cfg_attr(test, allow(...))]` line at the top of each
crate's `src/lib.rs`:

```bash
cargo clippy --workspace --lib --bins --message-format=json > /tmp/clippy.json
```

Each finding's `message.code.code` (for example `clippy::cast_possible_truncation`) and
its primary span were tallied per lint. A lint allowed at workspace level has more than
a handful of hits. Anything with 5 or fewer was fixed by hand instead where the crate
was ours to touch. Re-run the same command after a refactor to see whether an allow can
be tightened back to `warn` or removed.

The gate runs three clippy passes with warnings denied: workspace libraries
and binaries, all features and all targets, then no default features and all
targets. Use `tools/lint.sh` as the current definition of these checks.
The JSON command above is for diagnostics, not a replacement for the gate.

## Go, when a tooling component is written in it

No Go code exists in this workspace yet. When a round 4 tooling component is
easier to maintain in Go, the baseline is
[Google's Go style guide](https://google.github.io/styleguide/go/best-practices.html)
and [Effective Go](https://go.dev/doc/effective_go), enforced by `gofmt`, `goimports`,
`golangci-lint` with the personal go-projects configuration, and `comment-lint`
on comment prose. The same house prose rules apply. Runtime crates stay Rust.
