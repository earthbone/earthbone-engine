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

The original `Save` retains raw payloads for exact writing. An intermediate
engine world does not retain every unparsed vanilla payload. Saving that world
therefore does not establish that unresolved or unsupported data survives.
See [save.md](save.md#saved-forms-and-live-globals) and
[save.md](save.md#round-trip) for the current format and import boundaries.

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
