//! Turso backend using the 0.8.3 Rust API and documented WAL/MVCC contracts.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::{Error, Evidence, Result, Row, SCHEMA, TABLES};

/// Object-safe backend operations for a future shared or synced store.
pub trait HistoryStore {
    /// Replace one step and all its children in a single transaction.
    fn record<'a>(&'a self, evidence: &'a Evidence) -> StoreFuture<'a, WriteStats>;
    /// Run a saved query with bound arguments.
    fn query<'a>(&'a self, name: &'a str, args: &'a [String]) -> StoreFuture<'a, Vec<Row>>;
    /// Inspect rows or integrity with bound SQL parameters.
    fn sql<'a>(&'a self, sql: &'a str, params: &'a [String]) -> StoreFuture<'a, Vec<Row>>;
    /// Clear evidence for an explicitly requested rebuild.
    fn clear(&self) -> StoreFuture<'_, ()>;
}

/// Backend future without prescribing a network or execution runtime.
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + 'a>>;

/// Mutually exclusive Turso coordination modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// All openers hold a common OS lock, including readers and migrations.
    Locked,
    /// Experimental multi-process WAL with serialized write transactions.
    Multiprocess,
    /// Single-process MVCC with optimistic `BEGIN CONCURRENT` transactions.
    Mvcc,
}

impl Mode {
    /// Parse the explicit setting. Unknown modes are errors.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "locked" => Ok(Self::Locked),
            "multiprocess" => Ok(Self::Multiprocess),
            "mvcc" => Ok(Self::Mvcc),
            _ => Err(Error::Input(format!("unknown store mode {value}"))),
        }
    }
}

/// Counts include retried transactions and latency through successful commit.
#[derive(Debug, Default, serde::Serialize)]
pub struct WriteStats {
    /// Busy, locked, schema refresh or optimistic conflicts observed.
    pub conflicts: u64,
    /// Full transaction restarts.
    pub retries: u64,
    /// End-to-end time including retries, in microseconds.
    pub commit_us: u64,
}

/// In-process SQLite-format database. Keep the database alive with its connection.
pub struct TursoStore {
    database: turso::Database,
    connection: turso::Connection,
    mode: Mode,
    _lock: Option<File>,
}

impl TursoStore {
    /// Open a store, optionally applying migrations under an exclusive schema lock.
    pub async fn open(path: &Path, mode: Mode, migrate: bool) -> Result<Self> {
        let lock = if mode == Mode::Locked {
            Some(lock_file(path, "lock")?)
        } else {
            None
        };
        let schema_lock = if migrate && mode != Mode::Locked {
            Some(lock_file(path, "schema-lock")?)
        } else {
            None
        };
        let path_text = path
            .to_str()
            .ok_or_else(|| Error::Input("database path must be UTF-8".to_owned()))?;
        let started = Instant::now();
        let database = loop {
            match turso::Builder::new_local(path_text)
                .experimental_multiprocess_wal(mode == Mode::Multiprocess)
                .build()
                .await
            {
                Ok(database) => break database,
                Err(error) => {
                    let error = Error::Database(error);
                    if !retryable(&error) || started.elapsed() > Duration::from_secs(30) {
                        return Err(error);
                    }
                    tokio::time::sleep(Duration::from_millis(
                        10 + u64::from(std::process::id() % 17),
                    ))
                    .await;
                }
            }
        };
        let connection = database.connect()?;
        connection.busy_timeout(Duration::from_millis(100))?;
        if mode == Mode::Mvcc {
            let mut rows = connection.query("PRAGMA journal_mode=mvcc", ()).await?;
            while rows.next().await?.is_some() {}
        }
        let store = Self {
            database,
            connection,
            mode,
            _lock: lock,
        };
        if migrate {
            store.migrate().await?;
        }
        drop(schema_lock);
        Ok(store)
    }

    /// Open another connection to the same single-process MVCC database.
    pub fn connect(&self) -> Result<Self> {
        if self.mode != Mode::Mvcc {
            return Err(Error::Input(
                "extra connections require mvcc mode".to_owned(),
            ));
        }
        Ok(Self {
            database: self.database.clone(),
            connection: self.database.connect()?,
            mode: self.mode,
            _lock: None,
        })
    }

    async fn migrate(&self) -> Result<()> {
        if self
            .sql(
                "SELECT name FROM sqlite_schema WHERE name='schema_version'",
                &[],
            )
            .await?
            .is_empty()
        {
            self.connection.execute("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied TEXT NOT NULL)", ()).await?;
        }
        let rows = self
            .sql("SELECT MAX(version) AS version FROM schema_version", &[])
            .await?;
        let version = rows
            .first()
            .and_then(|r| r.get("version"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if version > 1 {
            return Err(Error::Input(format!(
                "unsupported schema version {version}"
            )));
        }
        if version == 0 {
            self.connection.execute("BEGIN IMMEDIATE", ()).await?;
            if let Err(error) = self.connection.execute_batch(SCHEMA).await {
                self.rollback().await?;
                return Err(error.into());
            }
            self.connection.execute("COMMIT", ()).await?;
        }
        Ok(())
    }

    /// Execute read SQL, used for integrity verification and custom analysis.
    pub async fn sql(&self, sql: &str, params: &[String]) -> Result<Vec<Row>> {
        let mut rows = self
            .connection
            .query(sql, turso::params_from_iter(params.iter().cloned()))
            .await?;
        let names = rows.column_names();
        let mut result = Vec::new();
        while let Some(value) = rows.next().await? {
            let mut row = Row::new();
            for (index, name) in names.iter().enumerate() {
                let value = match value.get_value(index)? {
                    turso::Value::Null => Value::Null,
                    turso::Value::Integer(n) => Value::from(n),
                    turso::Value::Real(n) => Value::from(n),
                    turso::Value::Text(s) => Value::from(s),
                    turso::Value::Blob(bytes) => Value::from(bytes),
                };
                row.insert(name.clone(), value);
            }
            result.push(row);
        }
        Ok(result)
    }

    /// Empty the evidence tables transactionally for a rebuild.
    pub async fn clear(&self) -> Result<()> {
        self.connection.execute("BEGIN IMMEDIATE", ()).await?;
        let result = async {
            for table in TABLES.iter().copied().chain(std::iter::once("runs")) {
                self.connection
                    .execute(format!("DELETE FROM {table}"), ())
                    .await?;
            }
            self.connection.execute("COMMIT", ()).await?;
            Ok::<_, turso::Error>(())
        }
        .await;
        if let Err(error) = result {
            self.rollback().await?;
            return Err(error.into());
        }
        Ok(())
    }

    async fn rollback(&self) -> Result<()> {
        if !self.connection.is_autocommit()? {
            self.connection.execute("ROLLBACK", ()).await?;
        }
        Ok(())
    }

    async fn insert_rows(&self, table: &str, rows: &[Row]) -> Result<()> {
        for chunk in rows.chunks(100) {
            let keys = chunk
                .iter()
                .flat_map(|row| row.keys())
                .collect::<BTreeSet<_>>();
            if keys.is_empty()
                || keys
                    .iter()
                    .any(|key| !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            {
                return Err(Error::Input("invalid row columns".to_owned()));
            }
            let columns = keys
                .iter()
                .map(|key| format!("\"{key}\""))
                .collect::<Vec<_>>()
                .join(",");
            let mut params = Vec::new();
            let mut slots = Vec::new();
            for row in chunk {
                let mut group = Vec::new();
                for key in &keys {
                    let value = row.get(*key).unwrap_or(&Value::Null);
                    params.push(match value {
                        Value::Null => turso::Value::Null,
                        Value::Bool(b) => turso::Value::Integer(i64::from(*b)),
                        Value::Number(n) => n.as_i64().map_or_else(
                            || turso::Value::Real(n.as_f64().unwrap_or(0.0)),
                            turso::Value::Integer,
                        ),
                        Value::String(s) => turso::Value::Text(s.clone()),
                        _ => turso::Value::Text(value.to_string()),
                    });
                    group.push(format!("?{}", params.len()));
                }
                slots.push(format!("({})", group.join(",")));
            }
            self.connection
                .execute(
                    format!("INSERT INTO {table} ({columns}) VALUES {}", slots.join(",")),
                    turso::params_from_iter(params),
                )
                .await?;
        }
        Ok(())
    }

    async fn replace(&self, evidence: &Evidence) -> Result<()> {
        let id = evidence.id()?;
        let mut fingerprints = self.sql("SELECT DISTINCT fingerprint FROM failures WHERE run=?1 AND fingerprint IS NOT NULL", &[id.to_owned()]).await?.into_iter().filter_map(|row| row.get("fingerprint").and_then(Value::as_str).map(str::to_owned)).collect::<BTreeSet<_>>();
        if let Some(rows) = evidence.rows.get("failures") {
            fingerprints.extend(rows.iter().filter_map(|row| {
                row.get("fingerprint")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }));
        }
        for table in TABLES {
            self.connection
                .execute(format!("DELETE FROM {table} WHERE run=?1"), [id])
                .await?;
        }
        self.connection
            .execute("DELETE FROM runs WHERE id=?1", [id])
            .await?;
        self.insert_rows("runs", std::slice::from_ref(&evidence.run))
            .await?;
        for (table, rows) in &evidence.rows {
            if !TABLES.contains(&table.as_str()) {
                return Err(Error::Input(format!("unknown table {table}")));
            }
            let values = rows
                .iter()
                .enumerate()
                .map(|(ordinal, value)| {
                    let mut value = value.clone();
                    value.insert(
                        "id".to_owned(),
                        Value::from(format!("{id}|{table}|{ordinal}")),
                    );
                    value.insert("run".to_owned(), Value::from(id));
                    value
                })
                .collect::<Vec<_>>();
            self.insert_rows(table, &values).await?;
        }
        for fingerprint in fingerprints {
            self.connection.execute("UPDATE failures SET first_commit=(SELECT r.commit_id FROM failures f JOIN runs r ON r.id=f.run WHERE f.fingerprint=?1 AND r.commit_id IS NOT NULL ORDER BY COALESCE(r.started,r.ended,''),r.id LIMIT 1),last_commit=(SELECT r.commit_id FROM failures f JOIN runs r ON r.id=f.run WHERE f.fingerprint=?1 AND r.commit_id IS NOT NULL ORDER BY COALESCE(r.started,r.ended,'') DESC,r.id DESC LIMIT 1) WHERE fingerprint=?1", [fingerprint]).await?;
        }
        Ok(())
    }

    async fn write(&self, evidence: &Evidence) -> Result<WriteStats> {
        evidence.id()?;
        let start = Instant::now();
        let mut stats = WriteStats::default();
        loop {
            let result = async {
                self.connection
                    .execute(
                        if self.mode == Mode::Mvcc {
                            "BEGIN CONCURRENT"
                        } else {
                            "BEGIN IMMEDIATE"
                        },
                        (),
                    )
                    .await?;
                self.replace(evidence).await?;
                self.connection.execute("COMMIT", ()).await?;
                Ok::<_, Error>(())
            }
            .await;
            match result {
                Ok(()) => {
                    stats.commit_us =
                        u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
                    return Ok(stats);
                }
                Err(error) => {
                    self.rollback().await?;
                    if !retryable(&error) || start.elapsed() > Duration::from_secs(30) {
                        return Err(error);
                    }
                    stats.conflicts += 1;
                    stats.retries += 1;
                    let jitter = u64::from(std::process::id() % 17);
                    tokio::time::sleep(Duration::from_millis(
                        (stats.retries.min(20) + jitter).min(30),
                    ))
                    .await;
                }
            }
        }
    }
}

impl HistoryStore for TursoStore {
    fn record<'a>(&'a self, evidence: &'a Evidence) -> StoreFuture<'a, WriteStats> {
        Box::pin(self.write(evidence))
    }

    fn query<'a>(&'a self, name: &'a str, args: &'a [String]) -> StoreFuture<'a, Vec<Row>> {
        Box::pin(async move {
            let (sql, params) = crate::queries::saved(name, args)?;
            self.sql(sql, &params).await
        })
    }

    fn sql<'a>(&'a self, sql: &'a str, params: &'a [String]) -> StoreFuture<'a, Vec<Row>> {
        Box::pin(TursoStore::sql(self, sql, params))
    }

    fn clear(&self) -> StoreFuture<'_, ()> {
        Box::pin(TursoStore::clear(self))
    }
}

fn lock_file(path: &Path, suffix: &str) -> Result<File> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{suffix}"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(name)?;
    file.lock()?;
    Ok(file)
}

fn retryable(error: &Error) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    matches!(error, Error::Database(_))
        && [
            "busy",
            "locked",
            "conflict",
            "schema updated",
            "schemaupdated",
        ]
        .iter()
        .any(|needle| text.contains(needle))
}
