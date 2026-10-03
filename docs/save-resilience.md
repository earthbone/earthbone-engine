# Save resilience requirements

The user requested stronger protection against missing mods and save damage
on 2026-09-20. These are acceptance requirements. They do not claim that the
current engine supports recovery play. Implementation tasks and their
dependencies live in [BACKLOG.csv](../BACKLOG.csv).

## Current boundary

The vanilla importer matches saved regular and light plugins by name. It
rejects missing plugins, duplicate names and changed regular/light kinds.
Several individual import passes validate before mutation. Whole-game atomic
restoration remains unfinished. These checks prevent certain bad imports but
do not implement missing-mod recovery.

The original `Save` retains raw payloads for exact writing. SOEW version 38
can retain caller-supplied opaque records under plugin-relative identities,
including exact payload bytes and links. The ESS importer does not yet populate
that table, and whole-game loading still rejects changed plugin provenance.
Saving an imported world therefore does not establish that unresolved vanilla
records survive. See [game.md](game.md#save-and-load) for this storage API and
the current import boundary.

The host's SAV-61 recovery coordinator preflights vanilla dependencies before
restoration. It refuses missing plugins or scripts, changed plugin kinds,
unimported active projectile and hazard changes, opaque PCON remainders and
PARW change forms. If restoration succeeds, it writes a native
generation in the caller's separate recovery store and leaves the source ESS
untouched. This does not retain arbitrary unsupported ESS change forms or
provide a complete recovered Skyrim session.

## Preserve identity and evidence

Native saves must retain the plugin and script provenance needed to compare
the saved content with the current loadout. Names, regular/light kind and
content fingerprints have distinct roles. Equal names alone do not prove
that a replaced or compacted plugin has the same records. For vanilla saves,
report provenance the format does not contain as unknown rather than inventing
it. Reordering plugins must not retarget saved identities.

Before mutating the world or VM, produce a deterministic report of missing,
changed and incompatible dependencies and the saved state they affect. Include
the original saved identity and the reason a link cannot be resolved. Offer
restoring the expected dependency set as a recovery route. A guessed record
match must never become an authoritative binding.

## Preserve unavailable state

Retain unresolved records, their links and payloads through native save/load,
including data whose execution semantics remain unknown. Keep these bytes
with their saved identity and provenance. They are deferred source data, not
a second authoritative live world. Import them into the shared form-backed
world only after validation.

Retain unavailable script instances, arrays, suspended calls and frame state
without executing them or replacing their links with permanent null values.
Do not silently discard queued work or redirect it to another object. Live
systems that depend on this state need an explicit unresolved result. Preserve
the distinction between a missing dependency and a record intentionally deleted
by the active load order.

When compatible content returns, validate and reconnect the preserved state
atomically. Resume eligible work once, preserving reference sharing and event
order. A changed script schema or changed plugin identity requires a supported
migration or continued suspension. A matching filename is insufficient.

## Make recovery explicit

Strict loading remains available. Recovery play is a separate, explicit choice
after the dependency report. If required player, world or execution state
cannot be used safely, refuse to start that session and explain the reason.
Do not promise that arbitrary mod removal preserves quest behavior.

Recovery saves use a distinct output and retain the source save. Make the
session's unresolved state visible when loading and saving it. Diagnostics
must distinguish missing content, unsupported decoding, incompatible state
and damaged bytes. Automatic deletion of orphaned or unresolved state is not
an acceptable default. No general cleaning operation is implied by this plan.

## Protect files and prove recovery

Write and validate a new save before replacing the destination. Flush file
contents and directory metadata at the required filesystem boundaries. A
failed write, failed validation or interrupted replacement must leave a valid
recoverable generation. Keep versioned backups under a documented retention
policy and never discard the last verified generation during rotation.

Acceptance scenarios must cover regular and light plugins, reordered slots,
same-name changed content, absent scripts and changed schemas. Remove a mod,
save and reload while it is absent, then restore compatible content. Verify
preserved payload bytes, identity links and shared arrays, unchanged unrelated
world state, and exactly-once resumed work. Malformed state must fail before
partial mutation or source replacement.

Inject I/O failures and terminate a separate writer process at each persistence
boundary. Check both the destination and backup generations on restart. Report
the tested filesystem and failure model. A passing simulated interruption test
does not prove durability on every device or filesystem.

## SAV-62 file replacement

`Game::write_file` writes the current native SOEG state. `Game::write_state_file`
validates and writes previously serialized native bytes against the same loaded
scripts and load order. Both validate world and VM state in scratch instances,
using the existing loaders without running callbacks or mutating the live game.
The small `Vm::validate_state` helper shares loaded script definitions with that
scratch VM. It is the only change outside the game and save crates.

`Save::write_file` writes an ESS container, checks its compressed change-form
payloads and requires exact re-encoding after read-back. It does not establish
that every opaque ESS payload has supported runtime semantics. Unsupported
payload decoding and whole-game import remain separate work.

Both APIs use `soe_save::atomic::write_validated` with this ordering:

1. Serialize and validate in memory before opening any output file.
2. Open and synchronize the existing parent directory to check support. Acquire
   the persistent `.NAME.soe-lock` advisory lock and reject a non-regular target.
3. Create an exclusive `.NAME.soe-stage-PID-COUNTER` sibling with mode 0600 on
   Unix. Write all bytes, synchronize the file, read it back and validate again.
   Synchronize the directory so the complete staging name is persistent.
4. If a prior destination exists, synchronize it, hard-link it under the staging
   name plus `.prior`, then synchronize the directory. Never reuse an existing
   staging or prior name.
5. Rename the validated staging file over the destination and synchronize the
   directory. Only after that succeeds, unlink this transaction's prior link
   and synchronize the directory again.

No existing save is opened for truncation. Cooperating writers serialize on
one persistent lock file, which must not be deleted during use. The operating
system releases its lock when the writer exits, including forced termination.
A retry creates fresh staging names and leaves earlier transaction evidence
untouched. Successful writes retain only the destination and lock file.

`WriteError::Io` reports the failed step, retained staging/prior paths and whether
replacement already happened. An error after replacement can leave the new
save at the destination. A staging path can contain partial or invalid bytes
and must be checked before recovery. Prior links are retained on errors until
a new generation's directory entry has been synchronized. If cleanup then
fails, the new destination is already synchronized. Recovery selection and
rotating generation retention are SAV-61 and SAV-63, not this transaction's
short-lived prior link.

The guarantee assumes a readable, durable prior destination, a stable trusted
parent directory, cooperating writers and a local filesystem that implements
advisory locking, hard links, atomic same-directory replacement and directory
synchronization. Unsupported operations fail before replacement. A first save
has no prior generation, so interruption can leave either no destination or a
complete new save. Relative paths resolve their existing parent before use.
Links and directories are rejected as destinations. Hostile directory mutation,
noncooperating writers, failing hardware and filesystems that ignore completed
sync operations are outside the tested model.

### Sources and validation

OpenMW's `apps/openmw/mwstate/statemanagerimp.cpp::saveGame` serializes into
memory first so serialization failure cannot truncate an existing save. Its
later direct `ofstream` write does not provide this replacement protocol.
FallrimTools' `src/main/java/resaver/ess/ESS.java::writeESS` preserves a backup
before opening the destination with `TRUNCATE_EXISTING`. That does not supply
the required atomic replacement or directory-sync ordering either. This code
uses their serialize-first and prior-generation ideas, with the ordering
specified by Linux [rename(2)](https://man7.org/linux/man-pages/man2/rename.2.html)
and [fsync(2)](https://man7.org/linux/man-pages/man2/fsync.2.html). Rust's
[File locking API](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)
provides the process-lifetime lock without a new dependency.

`crates/game/tests/atomic_save.rs` uses synthetic native and ESS files through
both public file writers. Its explicit matrix injects I/O errors before and
after all 17 persistence steps, 68 cases across the two formats. It also kills
separate writer processes at those same 68 boundaries and at 44 first-save
boundaries. Every restart loads the destination independently and checks any
retained prior links. Retrying preserves those links. Other checks cover a
partial staging write, corrupted read-back, compressed ESS containers, invalid
native world/VM bytes, missing loaded scripts, invalid compressed change data,
lock contention and lock release after termination. No user saves are used.

The failure hooks are enabled only by the `save-fault-tests` game feature,
which enables the save crate's `fault-injection` feature. There is no runtime
environment variable that enables a production fault. Run the matrix with:

```sh
cargo test -p soe-game --features save-fault-tests --test atomic_save
```

Set `SOE_ATOMIC_TEST_ROOT` to an existing owned test directory to choose its
filesystem. Each test creates and removes only its own exclusive child
directory. The acceptance runs use Linux tmpfs and btrfs. These are injected
operation failures and actual process terminations, not simulated disk cache
loss. They do not prove real power-loss behavior on arbitrary devices,
mount options or remote filesystems.

## SAV-63 verified generations

`GenerationStore` owns one existing, dedicated, trusted directory for native
SOEG generations. `Game::write_generation` publishes a new generation through
the SAV-62 atomic writer, then rotates retained files. `select_generation` is
read-only selection. `recover_generation` selects and validates before loading
any live world or VM state. These APIs do not implement missing-mod recovery,
ESS world export or a user-interface choice. SAV-61 owns that choice.

The persistent `.soe-generations.lock` serializes writers and selection.
Generation files are `generation-NNNNNNNNNNNNNNNNNNNN.soeb`, with a positive
20-digit decimal `u64` ID. New IDs are one greater than the largest existing
reserved filename, including damaged candidates. Exhaustion refuses a write.
Ordering never depends on timestamps or directory enumeration order. There is
no mutable index to repair after interruption.

Each SOEB version 1 envelope contains a generation ID, native byte length and
CRC32 over the header fields and payload. A filename/ID mismatch, truncated
body, trailing bytes or failed checksum is an integrity rejection. This
checksum detects accidental damage. It does not authenticate hostile edits.
The existing `crc32fast` game dependency supplies the checksum, with `cfg-if`
as its only dependency. No dependency or lockfile change was needed.

Integrity and loadability have separate meanings. A checksum-intact candidate
can still fail the scratch world/VM loaders under the current load order and
loaded scripts. Selection reports these as `Definitions`, separately from
`Integrity` damage and `Read` failures. It tries candidates newest first and
returns the first loadable generation plus the rejected newer candidates.
Selected bytes are owned in memory, so later file changes cannot alter them.
Rejected selection does not mutate the game. Selection never rotates or
removes files, including damaged evidence.

### Retention and interruption

The configured retention limit is 2 to 64 generations. After publishing a new
file, the writer reads and validates it again, synchronizes that file and the
store directory, then plans retention. It prefers the newest currently
loadable generations, followed by intact incompatible generations if slots
remain. This preserves a compatible older fallback ahead of a newer
incompatible file. The new generation must remain loadable before pruning
begins. A failed verification leaves every older generation untouched.

Only after this verification does pruning remove excess or damaged reserved
generation files, oldest first, with a directory sync after each deletion.
The successful write keeps at most the configured count. It can keep fewer
when fewer intact generations exist. It never deletes a retained generation.
The new durable generation remains protected throughout pruning. If there are
two loadable candidates, the policy retains at least two.

After pruning, cleanup removes exact protocol artifacts in sorted filename
order, synchronizing the directory after each removal. Eligible names are
`.generation-NNNNNNNNNNNNNNNNNNNN.soeb.soe-lock` and that generation's
`.soe-stage-PID-COUNTER` siblings, optionally ending in `.prior`. The numeric
components must parse at their specified widths. The store lock excludes any
active writer using those names. It is never removed. Unrelated files, other
lock namespaces and near-matching filenames are left alone. Non-regular
reserved paths are never followed or deleted. An error can therefore leave a
new published generation and unfinished cleanup, which a retry reports and
continues. Repeated interruptions can temporarily leave extra generations and
staging evidence. Bounded retention is the result of a successful rotation.

This requires the same local filesystem and synchronization behavior as
SAV-62, plus exclusive use of the reserved names inside the dedicated
directory. External mutation, independent atomic writers targeting generation
names, hostile symlink replacement and real device failure are outside this
cooperating-writer model. Rotation is the explicit retention policy for this
store, not a general save-cleaning operation.

### Host entry points and evidence

`PlayerNode::save_game_generation(directory, retained)` writes its shared
`Game`, including the latest controller pose. Godot's `ProjectSettings` expands
`user://` to an OS path. The directory must already exist.
`recover_game_generation` explicitly loads the selected native state, clears
stale melee contact state and synchronizes the host pose/resources. Both return
structured diagnostics with generation IDs represented as strings, preserving
the full `u64` range. Rejected candidates include their path and reason.

If resource installation fails after native load, the result reports
`state_loaded: true`, `ok: false` and stops automatic stepping. This does not
claim atomic replacement of every host resource. Definition failure leaves
native state unchanged, and no recovery callback is executed during selection.
Missing-mod continuation and full rendering reconciliation remain separate
features. The host bindings compile in the complete feature gate. Their
interactive Godot resource path is not covered by the synthetic Rust suite.

The sources read before this implementation were OpenMW's
`apps/openmw/mwstate/quicksavemanager.cpp`, `character.cpp` and the quicksave
caller in `statemanagerimp.cpp`, plus FallrimTools'
`src/main/java/resaver/ess/ESS.java::makeBackupFile`. OpenMW bounds quicksave
slots and chooses the oldest by timestamp. Its slot scan reads the SAVE/profile
header. FallrimTools makes timestamped copies with replacement allowed. Neither
implements this checksum/loadability distinction or durable verified pruning.
Godot's local `class_projectsettings.rst` documents path globalization. Its
saving-games tutorial writes through `FileAccess`, so the host here calls the
existing atomic native writer for the required persistence ordering.

`crates/game/tests/save_generations.rs` checks checksum damage even when the
changed payload remains individually loadable, damaged and incompatible
candidates, unchanged live state on rejection, bounded retention, counter
exhaustion and exact artifact ownership. Its explicit failure matrix covers
35 operation occurrences, including the nested atomic writer and each of
three generation deletions and three artifact deletions. Before/after tests
inject 70 I/O failures and kill 70 separate writer processes. Every restart
loads intact native generations independently, selects the newest loadable
one and completes a bounded retry. Further checks cover publication damage,
selection I/O failures and lock release after writer termination.

```sh
cargo test -p soe-game --features save-fault-tests --test save_generations
```

`SOE_GENERATION_TEST_ROOT` selects an existing owned directory for synthetic
fixtures. Tests create and remove only their own exclusive child directories.
The matrix runs on Linux tmpfs and btrfs. This tests injected errors and actual
process termination, not power-loss cache behavior or arbitrary storage
hardware. No user save is copied or changed.
