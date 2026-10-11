// SPDX-License-Identifier: GPL-3.0-or-later

//! Structured engine logging using tracing's event/field API and the
//! tracing-subscriber EnvFilter and reload layers (MIT, docs.rs).
//! Godot's GlobalScope logging reference defines the host routing contract.
//! Size rotation complements Godot's session rotation without a worker runtime.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Write as _};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{reload, EnvFilter, Layer, Registry};

const MAX_HISTORY: usize = 8192;
const MAX_PENDING: usize = 1024;
const FILE_BYTES: u64 = 4 * 1024 * 1024;
const FILE_BACKUPS: usize = 5;

static GLOBAL: OnceLock<LogControl> = OnceLock::new();
static CONTROLS: OnceLock<Mutex<Vec<Weak<Mutex<State>>>>> = OnceLock::new();

/// A collapsed log line. Timestamps are milliseconds since subscriber creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub first_ms: u64,
    pub last_ms: u64,
    pub level: Level,
    pub target: String,
    pub fields: BTreeMap<String, String>,
    pub count: u64,
}

impl fmt::Display for LogLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{} {} {}", self.first_ms, self.level, self.target)?;
        for (name, value) in &self.fields {
            write!(out, " {name}={value:?}")?;
        }
        write!(out, " count={}", self.count)
    }
}

/// Logging setup, filter and output failures.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("invalid SOE_LOG filter: {0}")]
    Filter(#[from] tracing_subscriber::filter::ParseError),
    #[error("cannot reload SOE_LOG filter: {0}")]
    Reload(#[from] reload::Error),
    #[error("log output: {0}")]
    Io(#[from] io::Error),
    #[error("a logging subscriber is already installed")]
    AlreadyInstalled,
}

type Sink = dyn Fn(&LogLine) + Send + Sync;

/// Runtime filter, output flushing and recent-window access shared by the host.
#[derive(Clone)]
pub struct LogControl {
    filter: reload::Handle<EnvFilter, Registry>,
    state: Arc<Mutex<State>>,
    sink: Arc<Sink>,
    epoch: Instant,
}

struct State {
    filter_text: String,
    pending: BTreeMap<String, LogLine>,
    ready: Vec<LogLine>,
    history: VecDeque<HitchLog>,
    file: Option<RotatingFile>,
}

#[derive(Clone)]
pub(crate) struct HitchLog {
    pub at: Instant,
    pub thread: u64,
    pub line: LogLine,
}

struct RotatingFile {
    directory: PathBuf,
    file: File,
    bytes: u64,
    limit: u64,
}

impl RotatingFile {
    fn open(directory: &Path, limit: u64) -> io::Result<Self> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join("engine.log");
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let bytes = file.metadata()?.len();
        Ok(Self {
            directory: directory.to_owned(),
            file,
            bytes,
            limit,
        })
    }

    fn write(&mut self, line: &LogLine) -> io::Result<()> {
        let text = format!("{line}\n");
        if self.bytes > 0 && self.bytes.saturating_add(text.len() as u64) > self.limit {
            self.file.flush()?;
            for index in (1..=FILE_BACKUPS).rev() {
                let source = self.directory.join(if index == 1 {
                    "engine.log".to_owned()
                } else {
                    format!("engine.{}.log", index - 1)
                });
                let destination = self.directory.join(format!("engine.{index}.log"));
                if source.exists() {
                    std::fs::rename(source, destination)?;
                }
            }
            self.file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(self.directory.join("engine.log"))?;
            self.bytes = 0;
        }
        self.file.write_all(text.as_bytes())?;
        self.file.flush()?;
        self.bytes += text.len() as u64;
        Ok(())
    }
}

fn parse_filter(text: &str) -> Result<EnvFilter, LogError> {
    Ok(EnvFilter::builder()
        .with_regex(false)
        .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
        .parse(text)?)
}

/// Build a subscriber without changing the process default.
///
/// # Errors
/// Rejects malformed filter directives or an unwritable log directory.
pub fn subscriber(
    filter: &str,
    directory: Option<&Path>,
    sink: impl Fn(&LogLine) + Send + Sync + 'static,
) -> Result<(impl Subscriber + Send + Sync, LogControl), LogError> {
    let (filter_layer, filter_handle) = reload::Layer::new(parse_filter(filter)?);
    let control = LogControl {
        filter: filter_handle,
        state: Arc::new(Mutex::new(State {
            filter_text: filter.to_owned(),
            pending: BTreeMap::new(),
            ready: Vec::new(),
            history: VecDeque::new(),
            file: directory
                .map(|path| RotatingFile::open(path, FILE_BYTES))
                .transpose()?,
        })),
        sink: Arc::new(sink),
        epoch: Instant::now(),
    };
    if let Ok(mut controls) = CONTROLS.get_or_init(|| Mutex::new(Vec::new())).lock() {
        controls.retain(|control| control.strong_count() != 0);
        controls.push(Arc::downgrade(&control.state));
    }
    let subscriber = Registry::default()
        .with(filter_layer)
        .with(LogLayer(control.clone()));
    Ok((subscriber, control))
}

/// Install the engine subscriber using SOE_LOG, defaulting to info.
///
/// # Errors
/// Returns filter, file or duplicate-subscriber errors without replacing a subscriber.
pub fn install(
    directory: &Path,
    sink: impl Fn(&LogLine) + Send + Sync + 'static,
) -> Result<(), LogError> {
    let filter = std::env::var("SOE_LOG").unwrap_or_else(|_| "info".to_owned());
    let (subscriber, control) = subscriber(&filter, Some(directory), sink)?;
    tracing::subscriber::set_global_default(subscriber).map_err(|_| LogError::AlreadyInstalled)?;
    GLOBAL.set(control).map_err(|_| LogError::AlreadyInstalled)
}

/// The host-installed process logging control, when available.
pub fn global() -> Option<&'static LogControl> {
    GLOBAL.get()
}

impl LogControl {
    /// Elapsed milliseconds on the logging clock, used for hitch windows.
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// The active SOE_LOG directive text.
    pub fn filter(&self) -> String {
        self.state
            .lock()
            .map(|state| state.filter_text.clone())
            .unwrap_or_default()
    }

    /// Replace the filter atomically. Invalid input retains the current filter.
    ///
    /// # Errors
    /// Returns a parse or reload error.
    pub fn set_filter(&self, text: &str) -> Result<(), LogError> {
        self.filter.reload(parse_filter(text)?)?;
        if let Ok(mut state) = self.state.lock() {
            text.clone_into(&mut state.filter_text);
        }
        Ok(())
    }

    #[cfg(test)]
    fn record(&self, line: LogLine) {
        self.record_at(
            self.epoch + std::time::Duration::from_millis(line.first_ms),
            line,
        );
    }

    fn record_at(&self, at: Instant, line: LogLine) {
        if let Ok(mut state) = self.state.lock() {
            if state.history.len() == MAX_HISTORY {
                state.history.pop_front();
            }
            state.history.push_back(HitchLog {
                at,
                thread: crate::thread_id(),
                line: line.clone(),
            });
            let key = key(&line);
            if let Some(previous) = state.pending.get_mut(&key) {
                if previous.first_ms / 1000 == line.first_ms / 1000 {
                    previous.last_ms = line.last_ms;
                    previous.count = previous.count.saturating_add(1);
                    return;
                }
            }
            if let Some(previous) = state.pending.remove(&key) {
                state.ready.push(previous);
            }
            if state.pending.len() == MAX_PENDING {
                if let Some((_, oldest)) = state.pending.pop_first() {
                    state.ready.push(oldest);
                }
            }
            state.pending.insert(key, line);
            // CONTEXT: The host drains every frame, but a blocked host must not
            // permit a worker to grow the output queue without a bound.
            if state.ready.len() > MAX_HISTORY {
                state.ready.remove(0);
            }
        }
    }

    /// Flush completed one-second buckets. The host calls this each frame.
    ///
    /// # Errors
    /// Returns an output error. Host delivery still occurs if file output fails.
    pub fn flush(&self) -> io::Result<()> {
        self.flush_at(self.now_ms(), false)
    }

    /// Flush the current bucket too, before shutdown or an explicit diagnostic dump.
    ///
    /// # Errors
    /// Returns an output error, while still delivering messages to the host.
    pub fn flush_all(&self) -> io::Result<()> {
        self.flush_at(self.now_ms(), true)
    }

    fn flush_at(&self, now: u64, all: bool) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log state poisoned"))?;
        let keys: Vec<_> = state
            .pending
            .iter()
            .filter(|(_, line)| all || line.first_ms / 1000 < now / 1000)
            .map(|(key, _)| key.clone())
            .collect();
        for key in keys {
            if let Some(line) = state.pending.remove(&key) {
                state.ready.push(line);
            }
        }
        let mut lines = std::mem::take(&mut state.ready);
        lines.sort_by(|a, b| {
            a.first_ms
                .cmp(&b.first_ms)
                .then_with(|| key(a).cmp(&key(b)))
        });
        let mut error = None;
        if let Some(file) = &mut state.file {
            for line in &lines {
                if let Err(err) = file.write(line) {
                    error = Some(err);
                }
            }
        }
        drop(state);
        for line in &lines {
            (self.sink)(line);
        }
        error.map_or(Ok(()), Err)
    }

    /// Collapsed retained log lines within an inclusive hitch window.
    /// Includes the current unflushed second, preserving each message's window membership.
    pub fn window(&self, start_ms: u64, end_ms: u64) -> Vec<LogLine> {
        self.select_window(|entry| (start_ms..=end_ms).contains(&entry.line.first_ms))
            .into_iter()
            .map(|entry| entry.line)
            .collect()
    }

    fn select_window(&self, contains: impl Fn(&HitchLog) -> bool) -> Vec<HitchLog> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        let mut lines: BTreeMap<(u64, String), HitchLog> = BTreeMap::new();
        for entry in state.history.iter().filter(|entry| contains(entry)) {
            let line = &entry.line;
            let at = entry.at;
            let thread = entry.thread;
            let entry = lines
                .entry((line.first_ms / 1000, key(line)))
                .or_insert_with(|| {
                    let mut copy = entry.clone();
                    copy.line.count = 0;
                    copy
                });
            if entry.at > at {
                entry.at = at;
                entry.thread = thread;
                entry.line.first_ms = line.first_ms;
            }
            entry.line.last_ms = entry.line.last_ms.max(line.last_ms);
            entry.line.count += 1;
        }
        let mut lines: Vec<_> = lines.into_values().collect();
        lines.sort_by_key(|entry| entry.at);
        lines
    }

    /// Write the PRF-18 log window, including messages awaiting output flushing.
    ///
    /// # Errors
    /// Returns the file write error without changing the log history.
    pub fn export_window(&self, path: &Path, start_ms: u64, end_ms: u64) -> io::Result<()> {
        let mut text = format!("PRF-18 logs {start_ms} to {end_ms} ms\n");
        for line in self.window(start_ms, end_ms) {
            let _ = writeln!(text, "{line}");
        }
        std::fs::write(path, text)
    }
}

pub(crate) fn hitch_window(start: Instant, end: Instant) -> Vec<HitchLog> {
    let Ok(mut controls) = CONTROLS.get_or_init(|| Mutex::new(Vec::new())).lock() else {
        return Vec::new();
    };
    controls.retain(|control| control.strong_count() != 0);
    let mut lines: BTreeMap<(u64, String), HitchLog> = BTreeMap::new();
    for entry in controls
        .iter()
        .filter_map(Weak::upgrade)
        .filter_map(|state| {
            state.lock().ok().map(|state| {
                state
                    .history
                    .iter()
                    .filter(|entry| (start..=end).contains(&entry.at))
                    .cloned()
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
    {
        let line = &entry.line;
        let bucket = (line.first_ms / 1000, key(line));
        let saved = lines.entry(bucket).or_insert_with(|| {
            let mut copy = entry.clone();
            copy.line.count = 0;
            copy
        });
        if entry.at < saved.at {
            saved.at = entry.at;
            saved.thread = entry.thread;
            saved.line.first_ms = line.first_ms;
        }
        saved.line.last_ms = saved.line.last_ms.max(line.last_ms);
        saved.line.count = saved.line.count.saturating_add(line.count);
    }
    let mut entries = lines.into_values().collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.at);
    entries
}

fn key(line: &LogLine) -> String {
    format!("{} {} {:?}", line.level, line.target, line.fields)
}

#[derive(Default, Clone)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

struct LogLayer(LogControl);

impl<S: Subscriber + for<'lookup> LookupSpan<'lookup>> Layer<S> for LogLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut fields = Fields::default();
            attrs.record(&mut fields);
            span.extensions_mut().insert(fields);
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            if let Some(fields) = span.extensions_mut().get_mut::<Fields>() {
                values.record(fields);
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut fields = Fields::default();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                if let Some(context) = span.extensions().get::<Fields>() {
                    fields.0.extend(context.0.clone());
                }
            }
        }
        event.record(&mut fields);
        fields
            .0
            .entry("system".to_owned())
            .or_insert_with(|| event.metadata().target().to_owned());
        let at = Instant::now();
        let now = at.duration_since(self.0.epoch).as_millis() as u64;
        self.0.record_at(
            at,
            LogLine {
                first_ms: now,
                last_ms: now,
                level: *event.metadata().level(),
                target: event.metadata().target().to_owned(),
                fields: fields.0,
                count: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn captured(
        filter: &str,
    ) -> (
        impl Subscriber + Send + Sync,
        LogControl,
        Arc<Mutex<Vec<LogLine>>>,
    ) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        let (subscriber, control) = subscriber(filter, None, move |line| {
            sink.lock().unwrap().push(line.clone());
        })
        .unwrap();
        (subscriber, control, lines)
    }

    fn line(time: u64, actor: &str) -> LogLine {
        LogLine {
            first_ms: time,
            last_ms: time,
            level: Level::ERROR,
            target: "game::melee".to_owned(),
            fields: BTreeMap::from([
                ("actor".to_owned(), actor.to_owned()),
                ("message".to_owned(), "weapon not equipped".to_owned()),
            ]),
            count: 1,
        }
    }

    #[test]
    fn tool_06_hitch_window_uses_exact_instants_across_clock_epochs() {
        let (subscriber, control, _) = captured("info");
        let cutoff = control.epoch + std::time::Duration::from_nanos(7_000_000_123);
        let end = cutoff + std::time::Duration::from_secs(6);
        for at in [
            cutoff
                .checked_sub(std::time::Duration::from_nanos(1))
                .unwrap(),
            cutoff,
            end,
            end + std::time::Duration::from_nanos(1),
        ] {
            let millis = at.duration_since(control.epoch).as_millis() as u64;
            control.record_at(at, line(millis, "14"));
        }
        tracing::subscriber::with_default(subscriber, || {
            let logs = hitch_window(cutoff, end);
            assert_eq!(
                logs.iter().map(|entry| entry.at).collect::<Vec<_>>(),
                [cutoff, end]
            );
            assert!(logs.iter().all(|entry| entry.line.count == 1));
        });
    }

    #[test]
    fn tool_06_hitch_window_orders_concurrent_message_timestamps() {
        let (subscriber, control, _) = captured("info");
        let start = control.epoch;
        control.record_at(start + std::time::Duration::from_millis(20), line(20, "14"));
        control.record_at(start + std::time::Duration::from_millis(10), line(10, "14"));
        tracing::subscriber::with_default(subscriber, || {
            let logs = hitch_window(start, start + std::time::Duration::from_secs(1));
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].at, start + std::time::Duration::from_millis(10));
            assert_eq!(
                (
                    logs[0].line.first_ms,
                    logs[0].line.last_ms,
                    logs[0].line.count
                ),
                (10, 20, 2)
            );
        });
    }

    #[test]
    fn tool_06_system_filter_reloads_and_invalid_input_preserves_filter() {
        let (subscriber, control, lines) = captured("warn,game::ai=info,papyrus=error");
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "game::ai", actor = "14", "enabled ai");
            tracing::info!(target: "papyrus", "hidden script");
            tracing::warn!(target: "game::melee", "enabled warning");
            control.set_filter("error,papyrus=info").unwrap();
            assert!(control.set_filter("papyrus=invalid").is_err());
            assert_eq!(control.filter(), "error,papyrus=info");
            tracing::info!(target: "game::ai", "hidden ai");
            tracing::info!(target: "papyrus", script = "Tool06", "enabled script");
        });
        control.flush_all().unwrap();
        let mut messages: Vec<_> = lines
            .lock()
            .unwrap()
            .iter()
            .map(|line| line.fields["message"].clone())
            .collect();
        messages.sort();
        assert_eq!(
            messages,
            ["enabled ai", "enabled script", "enabled warning"]
        );
    }

    #[test]
    fn tool_06_five_levels_and_structured_span_fields() {
        let (subscriber, control, lines) = captured("trace");
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "actor",
                system = "game",
                form_id = "0:000014",
                actor = "Player",
                plugin = "Fixture.esp",
                script = "Tool06"
            );
            let _guard = span.enter();
            tracing::error!("error");
            tracing::warn!("warn");
            tracing::info!("info");
            tracing::debug!("debug");
            tracing::trace!("trace");
        });
        control.flush_all().unwrap();
        let lines = lines.lock().unwrap();
        let expected = if cfg!(debug_assertions) { 5 } else { 3 };
        assert_eq!(lines.len(), expected);
        for line in lines.iter() {
            assert_eq!(line.fields["form_id"], "0:000014");
            assert_eq!(line.fields["plugin"], "Fixture.esp");
            assert_eq!(line.fields["actor"], "Player");
            assert_eq!(line.fields["script"], "Tool06");
            assert_eq!(line.fields["system"], "game");
        }
    }

    #[test]
    fn tool_06_release_debug_and_trace_arguments_are_not_evaluated() {
        let (subscriber, control, _) = captured("trace");
        let count = std::sync::atomic::AtomicU64::new(0);
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(
                value = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                "debug"
            );
            tracing::trace!(
                value = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                "trace"
            );
        });
        control.flush_all().unwrap();
        assert_eq!(
            count.load(std::sync::atomic::Ordering::Relaxed),
            if cfg!(debug_assertions) { 2 } else { 0 }
        );
    }

    #[test]
    fn tool_06_identical_messages_collapse_per_second_with_actor_identity() {
        let (_, control, lines) = captured("trace");
        for time in [0, 10, 990] {
            control.record(line(time, "14"));
        }
        control.record(line(990, "15"));
        control.flush_at(999, false).unwrap();
        assert!(lines.lock().unwrap().is_empty());
        control.flush_at(1000, false).unwrap();
        let first = lines.lock().unwrap().clone();
        assert_eq!(
            first.iter().map(|line| line.count).collect::<Vec<_>>(),
            [3, 1]
        );
        control.record(line(1001, "14"));
        control.flush_at(2000, false).unwrap();
        assert_eq!(lines.lock().unwrap().last().unwrap().count, 1);
    }

    #[test]
    fn tool_06_shutdown_flushes_last_bucket_and_sink_can_reenter() {
        let handle = Arc::new(Mutex::new(None::<LogControl>));
        let copy = handle.clone();
        let (subscriber, control) = subscriber("info", None, move |_| {
            assert!(!copy
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .window(0, u64::MAX)
                .is_empty());
        })
        .unwrap();
        *handle.lock().unwrap() = Some(control.clone());
        tracing::subscriber::with_default(subscriber, || tracing::info!("last"));
        control.flush_all().unwrap();
        assert!(control.state.lock().unwrap().pending.is_empty());
    }

    #[test]
    fn tool_06_file_rotates_with_all_levels_and_escaped_lines() {
        let directory = tempfile::tempdir().unwrap();
        let mut file = RotatingFile::open(directory.path(), 1).unwrap();
        for level in [
            Level::ERROR,
            Level::WARN,
            Level::INFO,
            Level::DEBUG,
            Level::TRACE,
        ] {
            let mut line = line(0, "14");
            line.level = level;
            line.fields.insert("message".to_owned(), "a\nb".to_owned());
            file.write(&line).unwrap();
        }
        let contents: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
            .collect();
        assert_eq!(contents.len(), 5);
        for level in ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"] {
            assert!(contents.iter().any(|text| text.contains(level)));
        }
        assert!(contents.iter().all(|text| text.lines().count() == 1));
        for _ in 0..20 {
            file.write(&line(0, "14")).unwrap();
        }
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            FILE_BACKUPS + 1
        );
    }

    #[test]
    fn tool_06_hitch_window_includes_pending_messages_only_inside_boundaries() {
        let (_, control, _) = captured("info");
        for time in [10, 20, 30, 1001] {
            control.record(line(time, "14"));
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hitch.log");
        control.export_window(&path, 20, 30).unwrap();
        assert_eq!(control.window(20, 30)[0].count, 2);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("20 ERROR game::melee"));
        assert!(text.contains("count=2"));
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn tool_06_history_and_pending_memory_are_bounded() {
        let (_, control, _) = captured("info");
        for number in 0..MAX_HISTORY + 10 {
            control.record(line(number as u64, &number.to_string()));
        }
        let state = control.state.lock().unwrap();
        assert_eq!(state.history.len(), MAX_HISTORY);
        assert!(state.pending.len() <= MAX_PENDING);
        assert!(state.ready.len() <= MAX_HISTORY);
    }

    #[test]
    fn tool_06_unwritable_output_reports_error_and_keeps_host_routing() {
        let directory = tempfile::tempdir().unwrap();
        let file_path = directory.path().join("not-a-directory");
        std::fs::write(&file_path, "file").unwrap();
        assert!(matches!(
            subscriber("info", Some(&file_path), |_| {}),
            Err(LogError::Io(_))
        ));
        let (_, control, lines) = captured("info");
        let mut file = RotatingFile::open(directory.path(), 1).unwrap();
        file.write(&line(0, "14")).unwrap();
        file.directory = file_path;
        control.state.lock().unwrap().file = Some(file);
        control.record(line(1, "15"));
        assert!(control.flush_all().is_err());
        assert_eq!(lines.lock().unwrap().len(), 1);
    }

    #[test]
    fn tool_06_print_lint_rejects_library_output_and_exempts_test_and_binary_output() {
        let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let path = directory.path();
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(path.join("Cargo.toml"), "[workspace]\n[package]\nname = \"tool-06-print-lint\"\nversion = \"0.0.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(
            path.join("src/lib.rs"),
            "pub fn output() { println!(\"stdout\"); eprintln!(\"stderr\"); }\n",
        )
        .unwrap();
        let output = std::process::Command::new("cargo")
            .args(["clippy", "--offline", "--manifest-path"])
            .arg(path.join("Cargo.toml"))
            .args([
                "--lib",
                "--",
                "-D",
                "clippy::print_stdout",
                "-D",
                "clippy::print_stderr",
            ])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let errors = String::from_utf8_lossy(&output.stderr);
        assert!(
            errors.contains("print_stdout") && errors.contains("print_stderr"),
            "{errors}"
        );
        std::fs::write(path.join("src/lib.rs"), "#[cfg(test)] mod tests { #[test] fn output() { println!(\"test\"); eprintln!(\"test\"); } }\n").unwrap();
        std::fs::write(
            path.join("src/main.rs"),
            "fn main() { println!(\"binary\"); eprintln!(\"binary\"); }\n",
        )
        .unwrap();
        let output = std::process::Command::new("cargo")
            .args(["clippy", "--offline", "--manifest-path"])
            .arg(path.join("Cargo.toml"))
            .args([
                "--lib",
                "--",
                "-D",
                "clippy::print_stdout",
                "-D",
                "clippy::print_stderr",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = std::process::Command::new("cargo")
            .args(["clippy", "--offline", "--manifest-path"])
            .arg(path.join("Cargo.toml"))
            .args(["--all-targets", "--", "-D", "warnings"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let gate = include_str!("../../../tools/lint.sh");
        assert!(gate.contains("cargo clippy --workspace --lib -- -D warnings -D clippy::print_stdout -D clippy::print_stderr"));
    }
}
