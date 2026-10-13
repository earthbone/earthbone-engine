CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS runs (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('build','tier1','clippy-prod','clippy-all','clippy-nodef','lint','installed','play-path','gate','fixer','pass')), branch TEXT, commit_id TEXT,
 parents TEXT, started TEXT, ended TEXT, exit_code INTEGER, cores INTEGER,
 ram_bytes INTEGER, nvme_free_bytes INTEGER, games_free_bytes INTEGER, gpu TEXT,
 driver TEXT, attempt INTEGER, log_path TEXT, record_path TEXT, skyrim_data TEXT, test_save TEXT,
 test_skyui TEXT, display_mode TEXT, metadata TEXT
);
CREATE TABLE IF NOT EXISTS tests (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, package TEXT, binary TEXT, name TEXT,
 status TEXT CHECK(status IN ('pass','fail','skip','ignored','killed','timeout','retry-one-thread')), reason TEXT, duration REAL, backlog_row TEXT, skyrim_data TEXT,
 test_save TEXT, test_skyui TEXT, display_mode TEXT
);
CREATE TABLE IF NOT EXISTS failures (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, test TEXT, message TEXT, file TEXT,
 line INTEGER, fingerprint TEXT, first_commit TEXT, last_commit TEXT
);
CREATE TABLE IF NOT EXISTS compile (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, unit TEXT, severity TEXT, error_code TEXT,
 message TEXT, file TEXT, line INTEGER
);
CREATE TABLE IF NOT EXISTS resources (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, process TEXT, binary TEXT, test TEXT,
 peak_rss_kib INTEGER, seconds REAL, earlyoom INTEGER, command TEXT, timestamp TEXT
);
CREATE TABLE IF NOT EXISTS play_checks (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, check_name TEXT, owner_row TEXT,
 pass INTEGER, status TEXT, stop TEXT, elapsed REAL, numbers TEXT
);
CREATE TABLE IF NOT EXISTS agents (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, rid TEXT, name TEXT, model TEXT,
 kind TEXT, minutes REAL, exit_code INTEGER, capacity_stops INTEGER, retries INTEGER,
 tokens TEXT, reply_status TEXT, commit_id TEXT, fixed TEXT, left TEXT
);
CREATE TABLE IF NOT EXISTS captures (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, commit_id TEXT, check_name TEXT, path TEXT
);
CREATE TABLE IF NOT EXISTS merges (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, tip TEXT, head TEXT, parent_tip TEXT, fixer TEXT,
 conflict INTEGER, message TEXT, timestamp TEXT
);
CREATE TABLE IF NOT EXISTS metrics (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, key TEXT, value TEXT, unit TEXT
);
CREATE TABLE IF NOT EXISTS evidence_lines (
 id TEXT PRIMARY KEY, run TEXT NOT NULL, ordinal INTEGER, text TEXT
);
CREATE INDEX IF NOT EXISTS runs_commit ON runs(commit_id);
CREATE INDEX IF NOT EXISTS tests_identity ON tests(package,binary,name,status);
CREATE INDEX IF NOT EXISTS tests_run ON tests(run);
CREATE INDEX IF NOT EXISTS metrics_run ON metrics(run);
CREATE INDEX IF NOT EXISTS evidence_run ON evidence_lines(run);
CREATE INDEX IF NOT EXISTS failures_run ON failures(run);
CREATE INDEX IF NOT EXISTS compile_run ON compile(run);
CREATE INDEX IF NOT EXISTS resources_run ON resources(run);
CREATE INDEX IF NOT EXISTS checks_run ON play_checks(run);
CREATE INDEX IF NOT EXISTS agents_run ON agents(run);
CREATE INDEX IF NOT EXISTS captures_run ON captures(run);
CREATE INDEX IF NOT EXISTS merges_run ON merges(run);
CREATE INDEX IF NOT EXISTS failures_fingerprint ON failures(fingerprint);
INSERT OR IGNORE INTO schema_version VALUES (1, CURRENT_TIMESTAMP);
