# Test history

`soe-test-history` stores harness evidence in an in-process Turso database.
The crate is GPL-3.0-or-later. Logs remain the record. Database files, source
logs, captures and installed game data are external artifacts, never committed.

## Build and use

The `store` feature is off by default because Turso has a substantial tree.
Build the CLI with `cargo build -p soe-test-history --features store`.
Parser fixtures also run without the feature.

```bash
export SOE_TEST_HISTORY=/path/to/history.db
target/debug/soe-test-history ingest '/path/to/log/**/*.log'
target/debug/soe-test-history rebuild --root /path/to/soe-run
target/debug/soe-test-history query flaky --format json
target/debug/soe-test-history query regressions COMMIT --format table
target/debug/soe-test-history query summary COMMIT
target/debug/soe-test-history schema
```

`ingest` expands globs and traverses directories in sorted order without
following symlinks. A source file is one run with a stable path identity.
Re-ingesting a growing file replaces its run and children in one transaction.
`rebuild` clears evidence and rediscovers `log`, `lead-evidence`,
`lead-gpu-logs`, `play`, `status.log`, `summary.json` and previously known files.
Images are stored as paths only. `evidence_lines` retains input lines,
including lines that the current parsers do not interpret. Inline image data
URLs and MCP image bytes are omitted, while their paths and metadata remain.

`record FILE`, or `record -` for stdin, accepts this JSON contract:

```json
{
  "run": {
    "id": "driver:step:attempt",
    "kind": "installed",
    "commit_id": "commit hash",
    "parents": ["parent hash"],
    "log_path": "/path/to/step.log"
  },
  "rows": {
    "tests": [{"package": "soe-game", "binary": "it", "name": "ply_03_walk", "status": "pass"}],
    "metrics": [{"key": "frames", "value": 40, "unit": "count"}]
  }
}
```

Callers supply a stable run id. Repeating that id replaces the whole step,
making retries idempotent. Text-log records generate a new id unless `--id`
is supplied. Child ids and run links are assigned by the store.
Unknown table names and invalid columns fail the transaction. Direct records
also have an append-only archive next to the database in `.db.records`.
Rebuild rediscovers those recorded paths. A future shared
or synced backend can implement the small, object-safe `HistoryStore` trait.
This crate adds no networking or multiplayer behavior.

For an existing text log, `record --log PATH --kind KIND --exit CODE` parses
it and accepts `--id`, `--commit`, `--parents`, `--branch`, `--started`,
`--ended`, `--driver` and `--attempt`. Recording a text log writes a sibling
`.history.json` manifest so rebuilding can recover the same facts. Direct JSON
records are archived before acknowledgement. Rebuild reads the whole archive
directory, preserving earlier records of an overwritten log. Unreferenced
pending records are excluded. Original harness logs remain the record.

## Coordination and dependency evidence

Context7 was read through its public HTTP API on 2026-10-13 using library
`/tursodatabase/turso`. Queries covered `Builder::new_local`, `connect`,
`execute`, `query`, transactions, rollback, MVCC, `BEGIN CONCURRENT` and
`experimental_multiprocess_wal`. The matching upstream documents were read
alongside the downloaded 0.8.3 Rust sources:

- [Rust API](https://github.com/tursodatabase/turso/blob/main/bindings/rust/README.md).
- [Transactions](https://github.com/tursodatabase/turso/blob/main/docs/sql-reference/statements/transactions.mdx).
- [Multi-process access](https://github.com/tursodatabase/turso/blob/main/docs/sql-reference/multiprocess-access.mdx).
- [Compatibility](https://github.com/tursodatabase/turso/blob/main/COMPAT.md).

The multi-process API and shared-memory format are experimental and may change
between releases. The shared WAL uses one writer slot across all processes,
independent reader snapshots and serialized checkpoints. Every opener must
enable the same option. It requires a supported 64-bit Unix platform and a
local filesystem with coherent mmap and POSIX locks. Mixed SQLite and Turso
processes are unsupported.

MVCC uses `PRAGMA journal_mode=mvcc` and `BEGIN CONCURRENT`, which checks
optimistic write conflicts at commit. Multi-process WAL does not support MVCC.
The documented modes cannot be combined on one file. The crate exposes three
mutually exclusive settings via `SOE_TEST_HISTORY_MODE` or `--mode`:

- `locked` holds one OS lock for the entire opener lifetime, including readers.
- `multiprocess`, the default after five passing stress trials, enables
  `experimental_multiprocess_wal(true)` and retries
  whole `BEGIN IMMEDIATE` transactions on busy or write conflicts.
- `mvcc` uses `BEGIN CONCURRENT` and permits sibling connections in one process.

Retries include bounded backoff and process jitter. A transaction rolls back
before retry. Unknown errors are returned immediately. Schema migrations use
a separate file lock and reject newer versions rather than overwriting them.

`cargo tree -p soe-test-history --features store` reported 108 newly resolved
packages, with 456 tree lines after adding the existing SHA-256 dependency
directly. Turso 0.8.3 pulls core, parser, extension macros,
SDK kit, sync SDK kit and sync engine even without its `sync` feature. The tree
includes ICU collation/data, AES/GCM/AEGIS crypto, regex, tracing, polling,
io-uring, UUID, bigdecimal, protobuf, concurrent collections and native build
tools such as cc, bindgen and clang-sys. FTS/Tantivy, mimalloc and HTTP sync
are disabled. No libSQL or SQLite C implementation is substituted for Turso.

## Schema and saved queries

Version 1 is in `crates/test-history/src/schema.sql`. Later versions append
migrations without changing old evidence identities. Tables are `runs`,
`tests`, `failures`, `compile`, `resources`, `play_checks`, `agents`, `captures`,
`merges`, `metrics`, `evidence_lines` and `schema_version`.

Run facts include branch, commit, parents, timing, exit, driver, attempt,
host capacity, both disks' free bytes, GPU and input/display settings.
Missing historical facts remain NULL. Tests preserve printed outcomes and
reasons. Test-name prefixes resolve through BACKLOG.csv before assigning an
owner. Failures keep location and normalized message fingerprints.
Diagnostics include warnings. Fingerprints are SHA-256 digests of normalized
messages, so large failure dumps do not create large index keys. RSS is stored
in KiB, binary time in seconds.
Play checks preserve PASS, FAIL, XFAIL and SKIP separately and retain all
printed JSON values. Nested JSON and key=value output also enter `metrics`.

Saved queries are `regressions`, `flaky`, `new-failures`, `memory`,
`slowest-tests`, `slowest-binaries`, `rows`, `play-path`, `agent-cost`,
`earlyoom`, `compile-errors` and `summary`. Commit arguments accept prefixes.
Regression queries compare exact package/binary/name identity against explicit
parent commits. Flakes require pass and fail at the same recorded commit.
New failures compare observations after the baseline's first recorded start.
The summary query returns the retained `summary.txt` lines for that commit,
including the driver's compile-unit, killed-binary and failure annotations.

## Harness and source attribution

`tools/lint.sh` makes one optional recorder call for each of the three clippy
passes and tier 1 when `SOE_TEST_HISTORY` is set. With it unset the original
commands run directly. With it set, output is also retained under
`${SOE_TEST_HISTORY}.logs`, or `SOE_TEST_HISTORY_LOG_DIR`. Set
`SOE_TEST_HISTORY_BIN` when the CLI is elsewhere. The gate never builds the
CLI and does not depend on recorder success. `tools/test-history-record.sh`
is the repository entry point for the lead's external harness wiring.

The local `engine-refs` corpus at
`/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs`
was searched for Turso and these harness formats. It contained no matching
parser. The parsers were written from the actual producers, read in place:
`.soe-run-tools/soe_run.py`, `.soe-run-tools/integrate_build.py` and
`G/godot-rss.sh`, plus Cargo output and TST-19 JSON observations. No game
format reader or engine state was changed. The RSS producer prints MiB,
converted to KiB on import. Synthetic fixtures cover these formats.

## Validation

`tools/test-history-stress.py` starts 16 writer and four reader processes
behind a common start barrier. Every transaction contains one run, test and
metric. Writers retain an independent acknowledgement ledger. Readers cycle
the saved queries for the full duration. The acceptance threshold, written
before running, is exact equality to all acknowledgements, one copy of each
row, no partial transaction, active readers and `PRAGMA integrity_check=ok`.
Throughput, conflicts, retries and p99 latency include complete transaction
time and backoff. Run the standard five one-minute trials with:

```bash
python3 tools/test-history-stress.py --binary target/debug/soe-test-history \
  --seconds 60 --trials 5 --directory /external/path/stress
```

The installed-log test uses `SOE_TEST_HISTORY_LOGS=G/log` and skips cleanly
when unset. It reads the external files in place and compares the saved query
to a summary snapshot captured before ingestion. The harness can update its
summary while this long corpus check runs. Synthetic tests plant a regression and
flake, verify fingerprint normalization, ingestion/rebuild idempotency,
rollback on invalid input and separate MVCC connections.

On the 24-core Linux host, Turso 0.8.3 passed all five 60-second trials with
16 writers and four readers. Every acknowledged row was present once,
every transaction was whole and every integrity check returned `ok`.

| Trial | Transactions | Transactions/s | Conflicts and retries | p99 commit ms | Reader queries |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 4,856 | 80.60 | 7,445 | 2,418.468 | 9,650 |
| 2 | 4,680 | 77.68 | 7,427 | 2,705.944 | 9,591 |
| 3 | 3,881 | 64.33 | 7,374 | 2,952.581 | 9,410 |
| 4 | 3,695 | 61.29 | 7,405 | 3,434.039 | 10,545 |
| 5 | 3,296 | 54.65 | 7,341 | 2,958.121 | 10,058 |

Totals were 20,408 committed transactions, 36,992 conflicts/retries and
49,254 saved reader queries. Two earlier five-trial sets also passed with
22,340 and 22,402 transactions and 50,166 and 50,178 reader queries.
The later trials overlapped real-log ingestion
on this shared host. These debug-build measurements establish the tested
workload's integrity, with substantial contention and second-scale tail
latency. They do not establish behavior after power loss, on network
filesystems, across Turso upgrades or with mixed SQLite openers. The locked
setting remains available. It must be used until a failing concurrent workload
has been reproduced and corrected.

The crate's 18 tests pass with `store` enabled, including the gated test's
clean return when its variable is unset. The default-feature run passes 11
parser tests. Strict crate clippy passes for all targets with and without
`store`. All 55 Python tool tests pass, including the three new hook tests.
The hook preserves stdout, stderr and the command exit code, including when
the recorder itself fails.

The full gate exits 1 at workspace formatting on two pre-existing long lines
in `crates/godot-host/src/godot_combat.rs:74`. No engine crate was changed.
Subsequent gate checks, including optional tools, were not reached. Standalone
prose lint, Taplo and Markdown lint passed. TST-20 remains blocked on this
integration gate failure, rather than declaring the gate complete.

The full gated corpus check passed over 13,364 sources in 5,576.87 seconds.
Its saved summary matched the captured integration-tip summary exactly.
An earlier 13,348-source attempt ingested every file but failed its final
comparison because the live harness changed summary.txt during ingestion.
Capturing the summary once before the scan corrected that race. No check was
removed. A persistent rebuild of G plus the earlyoom journal snapshot completed
with 16,659 recorded sources and historical status steps. The database is at
`/run/host/media/pengolodh/Data/soe-build/tmp/test-history-all.db`.

The completed database has 16,659 runs, 1,606,465 tests, 4,765 failures,
849 compile diagnostics, 141,950 resource observations, 1,357 play checks,
4,023 agent records, 21 capture paths, 275 merges, 8,680,130 metrics and
11,063,838 raw evidence lines. It occupies about 20 GB. A reflink snapshot of
the completed database reports no foreign-key violations. The original
database passes native `PRAGMA integrity_check`. Its summary query for
245c766d0 matches all 18 current integration summary lines, including the
ten installed failures. Resource observations include 1,126 earlyoom kills,
compile diagnostics include 332 warnings, and 1,055,812 tests have confirmed
backlog owners. The corpus, rebuild and database verification commands all
exited successfully.

Legacy files are source observations, rather than a reconstructed list of
distinct executions. Overwritten logs cannot supply their earlier contents.
Missing flags, exact start dates, host facts and per-test durations remain
NULL. Passing libtest output can hide an early return's printed skip reason.
RSS TSV rows identify a process and script, without an absolute timestamp or
Rust test identity. The store preserves those facts without guessing links.
Recorded commit parents and merge heads are enriched from immutable local
Git objects when available. First/last failure commits use recorded clocks,
with a stable source-id tie breaker when clocks are absent.

One large multi-process re-ingest hit the 30-second busy limit after a
committed source. The database still passed integrity_check and retained its
3,357 prior records. A fresh locked import completed with 3,365 records.
The shared harness append workload passed all three stress sets. Bulk maintenance
can use `--mode locked` after other openers have closed. Stress results do not
cover interrupted writers or power loss.
