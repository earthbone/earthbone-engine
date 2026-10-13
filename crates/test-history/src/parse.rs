//! Parsers for Cargo, integrate_build.py, soe_run.py, TST-19 and earlyoom logs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{row, Error, Evidence, Result, Row};

/// Canonical backlog identities used to validate test-name prefixes.
pub fn backlog(path: &Path) -> Result<BTreeSet<String>> {
    let text = std::fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter_map(|line| line.strip_prefix('"').and_then(|s| s.split('"').next()))
        .map(str::to_owned)
        .collect())
}

/// Decode one external file, retaining every line for later parser additions.
pub fn file(path: &Path, ids: &BTreeSet<String>, facts: &Row) -> Result<Evidence> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    if let Ok(mut evidence) = serde_json::from_str::<Evidence>(&text) {
        evidence.id()?;
        evidence.omit_image_bytes()?;
        evidence
            .run
            .entry("record_path".to_owned())
            .or_insert_with(|| json!(path.to_string_lossy()));
        return Ok(evidence);
    }
    let mut evidence = text_log(path, &text, ids)?;
    for (key, value) in facts {
        evidence.run.insert(key.clone(), value.clone());
    }
    let manifest = path.with_extension(format!(
        "{}.history.json",
        path.extension().and_then(|x| x.to_str()).unwrap_or("log")
    ));
    if manifest.is_file() {
        let saved: Evidence = serde_json::from_slice(&std::fs::read(manifest)?)?;
        evidence.run.extend(saved.run);
    }
    if let Some(captures) = evidence.rows.get_mut("captures") {
        for capture in captures {
            let Some(value) = capture.get("path").and_then(Value::as_str) else {
                continue;
            };
            if Path::new(value).is_relative() {
                if let Some(parent) = path.parent() {
                    if let Some(found) = [
                        parent.join(value),
                        parent.join("run").join(value),
                        parent.join("captures").join(value),
                    ]
                    .into_iter()
                    .find(|candidate| candidate.is_file())
                    {
                        capture.insert("path".to_owned(), json!(found.to_string_lossy()));
                    }
                }
            }
        }
    }
    for table in ["tests", "failures", "captures"] {
        if let Some(rows) = evidence.rows.get_mut(table) {
            for row in rows {
                if table == "tests" {
                    for key in ["skyrim_data", "test_save", "test_skyui", "display_mode"] {
                        if let Some(value) = evidence.run.get(key) {
                            row.insert(key.to_owned(), value.clone());
                        }
                    }
                } else if let Some(value) = evidence.run.get("commit_id") {
                    if table == "captures" {
                        row.insert("commit_id".to_owned(), value.clone());
                    } else {
                        row.insert("first_commit".to_owned(), value.clone());
                        row.insert("last_commit".to_owned(), value.clone());
                    }
                }
            }
        }
    }
    evidence.omit_image_bytes()?;
    Ok(evidence)
}

/// Expand paths, directories and globs in stable order without following symlinks.
pub fn discover(inputs: &[String]) -> Result<Vec<PathBuf>> {
    let mut paths = BTreeSet::new();
    for input in inputs {
        let entries = glob::glob(input).map_err(|e| Error::Input(e.to_string()))?;
        let mut matched = false;
        for entry in entries {
            let path = entry.map_err(|e| Error::Input(e.to_string()))?;
            matched = true;
            collect(&path, &mut paths, true)?;
        }
        if !matched {
            return Err(Error::Input(format!("no paths match {input}")));
        }
    }
    Ok(paths.into_iter().collect())
}

fn collect(path: &Path, paths: &mut BTreeSet<PathBuf>, explicit: bool) -> Result<()> {
    if path.components().any(|component| {
        ["converted-interface", "target", ".git"]
            .iter()
            .any(|name| component.as_os_str() == *name)
    }) {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            collect(&entry?.path(), paths, false)?;
        }
    } else if path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            [
                "log", "txt", "tsv", "jsonl", "last", "json", "stderr", "pending",
            ]
            .contains(&ext)
        })
        && !path.to_string_lossy().ends_with(".history.json")
        && (explicit
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pending")))
    {
        paths.insert(std::fs::canonicalize(path)?);
    }
    Ok(())
}

/// Hash normalized messages to bound the size of failure index keys.
pub fn fingerprint(message: &str) -> Result<String> {
    let pattern = Regex::new(r"(?i)0x[0-9a-f]+|[-+]?\d+(?:\.\d+)?(?:e[-+]?\d+)?")?;
    let normalized = pattern
        .replace_all(message, "#")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("{:x}", Sha256::digest(normalized.as_bytes())))
}

/// Parse synthetic or real text with source context, without inventing missing facts.
pub fn text_log(path: &Path, text: &str, ids: &BTreeSet<String>) -> Result<Evidence> {
    let source = path.to_string_lossy();
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let kind = kind(filename);
    let mut evidence = Evidence::new(&source, kind);
    evidence
        .run
        .insert("log_path".to_owned(), Value::from(source.as_ref()));
    let clean = Regex::new(r"\x1b\[[0-9;]*[A-Za-z]")?.replace_all(text, "");
    let mut parser = Parser::new(ids)?;
    let is_reply = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("last"));
    let is_events = filename.contains(".events")
        || clean
            .lines()
            .find(|line| !line.trim().is_empty())
            .and_then(|line| serde_json::from_str::<Value>(line).ok())
            .is_some_and(|value| {
                value["type"] == "thread.started" || value["type"] == "turn.started"
            });
    for (ordinal, line) in clean.lines().enumerate() {
        evidence.push(
            "evidence_lines",
            row(&[("ordinal", json!(ordinal + 1)), ("text", json!(line))]),
        );
        if is_events {
            parser.event(&mut evidence, line, ordinal)?;
        } else if !is_reply {
            parser.line(&mut evidence, line)?;
        }
    }
    parser.finish_failure(&mut evidence)?;
    if Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("last"))
    {
        parser.reply(&mut evidence, text, path);
    } else if is_events {
        parser.agent(&mut evidence, path)?;
    }
    if filename == "status.log" {
        status(&mut evidence, &clean)?;
    }
    evidence.omit_image_bytes()?;
    Ok(evidence)
}

fn kind(name: &str) -> &'static str {
    if name.starts_with("installed") {
        "installed"
    } else if name.contains("clippy-prod") {
        "clippy-prod"
    } else if name.contains("clippy-all") {
        "clippy-all"
    } else if name.contains("clippy-nodef") {
        "clippy-nodef"
    } else if name.contains("lint") {
        "lint"
    } else if name.contains("build")
        && Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
    {
        "build"
    } else if Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("last"))
        || name.contains("events")
    {
        "fixer"
    } else if name.contains("summary") || name == "status.log" {
        "pass"
    } else if name == "test.log" {
        "play-path"
    } else {
        "tier1"
    }
}

struct Parser<'a> {
    ids: &'a BTreeSet<String>,
    test: Regex,
    prefix: Regex,
    running: Regex,
    panic: Regex,
    compile: Regex,
    location: Regex,
    timing: Regex,
    kv: Regex,
    earlyoom: Regex,
    images: Regex,
    binary: String,
    package: String,
    current: Option<String>,
    failure: Option<usize>,
    failure_lines: Vec<String>,
    diagnostic: Option<usize>,
    tokens: Row,
    capacity: u64,
    skips: Row,
}

impl<'a> Parser<'a> {
    fn event(&mut self, evidence: &mut Evidence, line: &str, ordinal: usize) -> Result<()> {
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            evidence.push(
                "metrics",
                row(&[
                    ("key", json!("events.invalid_json")),
                    ("value", json!(ordinal)),
                ]),
            );
            return Ok(());
        };
        crate::omit_images(&mut value, &self.images)?;
        let event_type = value["type"].as_str().unwrap_or("unknown");
        flatten_numbers(evidence, &format!("event.{event_type}"), &value);
        if let Some(usage) = value.get("usage").and_then(Value::as_object) {
            for (key, value) in usage {
                let total = self
                    .tokens
                    .get(key)
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .saturating_add(value.as_u64().unwrap_or(0));
                self.tokens.insert(key.clone(), json!(total));
            }
        }
        if (value["type"] == "error"
            || value["type"] == "turn.failed"
            || value["item"]["type"] == "error")
            && value
                .to_string()
                .to_ascii_lowercase()
                .contains("at capacity")
        {
            self.capacity += 1;
        }
        Ok(())
    }

    fn new(ids: &'a BTreeSet<String>) -> Result<Self> {
        Ok(Self {
            ids,
            test: Regex::new(r"^test (\S+) \.\.\.\s*(ok|FAILED|ignored)?")?,
            prefix: Regex::new(r"(?:^|::)([a-z]{2,4})_(\d{2})_")?,
            running: Regex::new(
                r"Running (?:unittests (\S+) \(([^)]+)\)|(?:tests/)?(\S+) \(([^)]+)\))",
            )?,
            panic: Regex::new(r"panicked at (.*?):(\d+)(?::\d+)?:?")?,
            compile: Regex::new(r"^(error|warning)(?:\[([A-Za-z0-9_]+)\])?: (.*)")?,
            location: Regex::new(r"^\s*(?:-->|:::) (.*?):(\d+):\d+")?,
            timing: Regex::new(r"finished in ([0-9.]+)s")?,
            kv: Regex::new(r#"\b([A-Za-z][A-Za-z0-9_.-]*)=("[^"]*"|'[^']*'|[^\s,]+)"#)?,
            earlyoom: Regex::new(
                r#"(?i)(?:killed|sending SIGKILL to|sending SIGTERM to).*?(?:process )?(\d+).*?['"]([^'"]+)['"].*?(?:VmRSS|RSS)[=: ]+(\d+)(?:\s+(MiB|KiB))?"#,
            )?,
            images: Regex::new(r#"(?i)data:image/[^\s\"'<>]+"#)?,
            binary: String::new(),
            package: String::new(),
            current: None,
            failure: None,
            failure_lines: Vec::new(),
            diagnostic: None,
            tokens: Row::new(),
            capacity: 0,
            skips: Row::new(),
        })
    }

    fn line(&mut self, evidence: &mut Evidence, line: &str) -> Result<()> {
        if let Some(c) = self.running.captures(line) {
            let target = c.get(2).or_else(|| c.get(4)).map_or("", |x| x.as_str());
            let stem = Path::new(target)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(target);
            self.binary = stem
                .rsplit_once('-')
                .map_or(stem, |(name, _)| name)
                .to_owned();
            if self.binary.starts_with("soe_") {
                self.package = self.binary.replace('_', "-");
            } else if let Some(name) = Path::new(
                evidence
                    .run
                    .get("log_path")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("installed-"))
            .and_then(|s| s.strip_suffix(".log"))
            {
                self.package = name.to_owned();
            }
            self.current = None;
        }
        if let Some(c) = self.test.captures(line) {
            self.current = Some(c[1].to_owned());
            if let Some(status) = c.get(2) {
                self.test_result(evidence, &c[1], status.as_str(), line);
            }
        } else if ["ok", "FAILED", "ignored"].contains(&line.trim()) {
            if let Some(name) = self.current.clone() {
                self.test_result(evidence, &name, line.trim(), line);
            }
        }
        if let Some(name) = line
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" stdout ----"))
        {
            self.current = Some(name.to_owned());
            self.finish_failure(evidence)?;
        }
        if let Some((file, line_number)) = self
            .panic
            .captures(line)
            .map(|c| (c[1].to_owned(), c[2].parse::<u64>().ok()))
        {
            self.finish_failure(evidence)?;
            let test = self.current.as_ref().and_then(|name| {
                evidence.rows.get("tests").and_then(|rows| {
                    rows.iter()
                        .position(|r| r.get("name").and_then(Value::as_str) == Some(name))
                })
            });
            let id = evidence.id()?.to_owned();
            let value = row(&[
                (
                    "test",
                    test.map_or(Value::Null, |index| json!(format!("{id}|tests|{index}"))),
                ),
                ("message", json!(line)),
                ("file", json!(file)),
                ("line", json!(line_number)),
                ("fingerprint", json!(fingerprint(line)?)),
            ]);
            self.failure = Some(evidence.rows.get("failures").map_or(0, Vec::len));
            self.failure_lines.push(line.to_owned());
            evidence.push("failures", value);
        } else if !line.trim().is_empty()
            && !line.starts_with("note:")
            && !line.starts_with("failures:")
            && !line.starts_with("test result:")
            && !line.starts_with("---- ")
        {
            if self.failure.is_some() {
                self.failure_lines.push(line.to_owned());
            }
        } else {
            self.finish_failure(evidence)?;
        }
        if let Some(c) = self.compile.captures(line) {
            self.diagnostic = Some(evidence.rows.get("compile").map_or(0, Vec::len));
            let unit = Regex::new(r"could not compile `([^`]+)`")?
                .captures(line)
                .map(|c| c[1].to_owned())
                .unwrap_or_else(|| self.package.clone());
            evidence.push(
                "compile",
                row(&[
                    ("unit", json!(unit)),
                    ("severity", json!(&c[1])),
                    ("error_code", json!(c.get(2).map(|x| x.as_str()))),
                    ("message", json!(&c[3])),
                ]),
            );
        }
        if let Some(c) = self.location.captures(line) {
            if let Some(value) = evidence
                .rows
                .get_mut("compile")
                .and_then(|rows| self.diagnostic.and_then(|index| rows.get_mut(index)))
            {
                value.insert("file".to_owned(), json!(&c[1]));
                value.insert("line".to_owned(), json!(c[2].parse::<u64>().ok()));
                if let Some(unit) = c[1]
                    .strip_prefix("crates/")
                    .and_then(|s| s.split('/').next())
                {
                    value.insert("unit".to_owned(), json!(format!("soe-{unit}")));
                }
            }
        }
        if let Some(c) = self.timing.captures(line) {
            evidence.push(
                "resources",
                row(&[
                    ("binary", json!(self.binary)),
                    ("seconds", json!(c[1].parse::<f64>().ok())),
                ]),
            );
        }
        for c in self.kv.captures_iter(line) {
            evidence.push(
                "metrics",
                row(&[
                    ("key", json!(&c[1])),
                    ("value", json!(c[2].trim_matches(['\'', '"']))),
                    ("unit", json!(metric_unit(&c[1]))),
                ]),
            );
            if [
                "SKYRIM_DATA",
                "SOE_TEST_SAVE",
                "SOE_TEST_SKYUI",
                "DISPLAY",
                "SOE_PLAY_PATH",
            ]
            .contains(&&c[1])
            {
                let key = match &c[1] {
                    "SKYRIM_DATA" => "skyrim_data",
                    "SOE_TEST_SAVE" => "test_save",
                    "SOE_TEST_SKYUI" => "test_skyui",
                    _ => "display_mode",
                };
                evidence
                    .run
                    .insert(key.to_owned(), json!(c[2].trim_matches(['\'', '"'])));
            }
        }
        if line.to_ascii_lowercase().contains("skipping") || line.starts_with("SKIP ") {
            if let Some(name) = &self.current {
                self.skips.insert(name.clone(), json!(line));
                if let Some(test) = evidence.rows.get_mut("tests").and_then(|rows| {
                    rows.iter_mut()
                        .rev()
                        .find(|r| r.get("name").and_then(Value::as_str) == Some(name))
                }) {
                    test.insert("status".to_owned(), json!("skip"));
                    test.insert("reason".to_owned(), json!(line));
                }
            }
        }
        if line.contains("test-threads=1") || line.contains("one thread") {
            evidence.push(
                "metrics",
                row(&[("key", json!("retry.one_thread")), ("value", json!(line))]),
            );
            if line.to_ascii_lowercase().contains("retr") {
                self.test_result(
                    evidence,
                    self.current.as_deref().unwrap_or("<binary>"),
                    "retry-one-thread",
                    line,
                );
            }
        }
        if line.contains("process didn't exit successfully")
            || line.contains("timed out")
            || line.contains("timeout exceeded")
        {
            let status = if line.contains("signal:") {
                "killed"
            } else {
                "timeout"
            };
            self.test_result(
                evidence,
                self.current.as_deref().unwrap_or("<binary>"),
                status,
                line,
            );
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() >= 3
            && columns[0].parse::<u64>().is_ok()
            && columns[1].parse::<u64>().is_ok()
        {
            evidence.push(
                "resources",
                row(&[
                    ("process", json!(columns[0])),
                    (
                        "peak_rss_kib",
                        json!(columns[1]
                            .parse::<u64>()
                            .ok()
                            .and_then(|mib| mib.checked_mul(1024))),
                    ),
                    ("seconds", json!(columns[2].parse::<f64>().ok())),
                    ("command", json!(columns.get(3).copied())),
                    (
                        "binary",
                        json!(columns
                            .get(3)
                            .and_then(|command| command.split_whitespace().last())
                            .and_then(|name| Path::new(name).file_name())
                            .and_then(|name| name.to_str())),
                    ),
                ]),
            );
        }
        if let Some(c) = self.earlyoom.captures(line) {
            evidence.push(
                "resources",
                row(&[
                    ("earlyoom", json!(true)),
                    ("process", json!(&c[1])),
                    (
                        "command",
                        json!(line
                            .split("cmdline \"")
                            .nth(1)
                            .and_then(|value| value.strip_suffix('"'))
                            .unwrap_or(&c[2])),
                    ),
                    (
                        "peak_rss_kib",
                        json!(c[3].parse::<u64>().ok().and_then(|rss| rss.checked_mul(
                            if c.get(4)
                                .is_some_and(|unit| unit.as_str().eq_ignore_ascii_case("MiB"))
                            {
                                1024
                            } else {
                                1
                            }
                        ))),
                    ),
                    (
                        "timestamp",
                        json!(line.split("earlyoom").next().unwrap_or("")),
                    ),
                ]),
            );
        }
        if line.to_ascii_lowercase().contains("at capacity") {
            self.capacity += 1;
        }
        if let Some(index) = line.find('{') {
            if let Ok(mut value) = serde_json::from_str::<Value>(&line[index..]) {
                crate::omit_images(&mut value, &self.images)?;
                self.structured(evidence, &value, line);
            }
        }
        Ok(())
    }

    fn finish_failure(&mut self, evidence: &mut Evidence) -> Result<()> {
        if let Some(index) = self.failure.take() {
            let message = std::mem::take(&mut self.failure_lines).join("\n");
            if let Some(failure) = evidence
                .rows
                .get_mut("failures")
                .and_then(|rows| rows.get_mut(index))
            {
                failure.insert("fingerprint".to_owned(), json!(fingerprint(&message)?));
                failure.insert("message".to_owned(), json!(message));
            }
        }
        Ok(())
    }

    fn test_result(&self, evidence: &mut Evidence, name: &str, result: &str, reason: &str) {
        let status = match result {
            "ok" => "pass",
            "FAILED" => "fail",
            "ignored" => "ignored",
            _ => result,
        };
        let skip_reason = self.skips.get(name).and_then(Value::as_str);
        let status = if skip_reason.is_some() && status == "pass" {
            "skip"
        } else {
            status
        };
        let owner = self
            .prefix
            .captures(name)
            .map(|c| format!("{}-{}", c[1].to_ascii_uppercase(), &c[2]))
            .filter(|id| self.ids.contains(id));
        evidence.push(
            "tests",
            row(&[
                (
                    "package",
                    json!((!self.package.is_empty()).then_some(&self.package)),
                ),
                (
                    "binary",
                    json!((!self.binary.is_empty()).then_some(&self.binary)),
                ),
                ("name", json!(name)),
                ("status", json!(status)),
                ("reason", json!(skip_reason.unwrap_or(reason))),
                ("backlog_row", json!(owner)),
            ]),
        );
    }

    fn structured(&mut self, evidence: &mut Evidence, value: &Value, line: &str) {
        if let Some(check) = value["check"].as_str() {
            flatten(evidence, &format!("play.{check}"), &value["value"]);
        } else {
            flatten(evidence, "json", value);
        }
        if value.get("check").is_some() && line.contains("PLAY_PATH") {
            evidence.push(
                "play_checks",
                row(&[
                    ("check_name", value["check"].clone()),
                    ("owner_row", value["owner"].clone()),
                    ("pass", value["pass"].clone()),
                    ("status", value["status"].clone()),
                    ("stop", value["stop"].clone()),
                    ("elapsed", value["elapsed_s"].clone()),
                    ("numbers", value["value"].clone()),
                ]),
            );
            if let Some(path) = value["value"]["path"].as_str() {
                evidence.push(
                    "captures",
                    row(&[
                        ("check_name", value["check"].clone()),
                        ("path", json!(path)),
                    ]),
                );
            }
            if value["check"] == "window" {
                evidence
                    .run
                    .insert("display_mode".to_owned(), value["value"].clone());
            }
            if value["check"] == "gpu_adapter" {
                evidence
                    .run
                    .insert("gpu".to_owned(), value["value"]["adapter"].clone());
            }
        }
        if let Some(usage) = value.get("usage").and_then(Value::as_object) {
            for (key, value) in usage {
                let total = self.tokens.get(key).and_then(Value::as_u64).unwrap_or(0)
                    + value.as_u64().unwrap_or(0);
                self.tokens.insert(key.clone(), json!(total));
            }
        }
        if value["reason"] == "compiler-message" {
            let message = &value["message"];
            let span = message["spans"]
                .as_array()
                .and_then(|spans| spans.iter().find(|s| s["is_primary"] == true));
            evidence.push(
                "compile",
                row(&[
                    ("unit", value["target"]["name"].clone()),
                    ("severity", message["level"].clone()),
                    ("error_code", message["code"]["code"].clone()),
                    ("message", message["message"].clone()),
                    ("file", span.map_or(Value::Null, |s| s["file_name"].clone())),
                    (
                        "line",
                        span.map_or(Value::Null, |s| s["line_start"].clone()),
                    ),
                ]),
            );
        }
        if value["type"] == "test" && value.get("name").is_some() {
            if let Some(result) = value["event"].as_str() {
                if ["ok", "failed", "ignored"].contains(&result) {
                    self.test_result(
                        evidence,
                        value["name"].as_str().unwrap_or(""),
                        if result == "failed" { "FAILED" } else { result },
                        line,
                    );
                    if let Some(test) = evidence
                        .rows
                        .get_mut("tests")
                        .and_then(|rows| rows.last_mut())
                    {
                        test.insert("duration".to_owned(), value["exec_time"].clone());
                    }
                }
            }
        }
    }

    fn agent(&self, evidence: &mut Evidence, path: &Path) -> Result<()> {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .split(".events")
            .next()
            .unwrap_or("");
        evidence.push(
            "agents",
            row(&[
                (
                    "rid",
                    json!(path
                        .parent()
                        .and_then(Path::file_name)
                        .and_then(|s| s.to_str())),
                ),
                ("name", json!(name)),
                ("tokens", json!(self.tokens)),
                ("capacity_stops", json!(self.capacity)),
                ("retries", json!(u64::from(name.contains("retry")))),
            ]),
        );
        let reply = path.with_file_name(format!("{name}.last"));
        if reply.is_file()
            && !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".events-"))
        {
            if let Ok(value) = serde_json::from_slice::<Value>(&std::fs::read(reply)?) {
                if let Some(agent) = evidence
                    .rows
                    .get_mut("agents")
                    .and_then(|rows| rows.last_mut())
                {
                    add_reply(agent, &value);
                }
            }
        }
        Ok(())
    }

    fn reply(&self, evidence: &mut Evidence, text: &str, path: &Path) {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            let events = path.with_extension("events.jsonl");
            if !events.is_file() {
                let mut agent = row(&[
                    (
                        "rid",
                        json!(path
                            .parent()
                            .and_then(Path::file_name)
                            .and_then(|s| s.to_str())),
                    ),
                    ("name", json!(path.file_stem().and_then(|s| s.to_str()))),
                ]);
                add_reply(&mut agent, &value);
                evidence.push("agents", agent);
            }
        } else {
            evidence.push(
                "metrics",
                row(&[("key", json!("reply.invalid_json")), ("value", json!(text))]),
            );
        }
    }
}

fn add_reply(agent: &mut Row, value: &Value) {
    for (target, source) in [
        ("reply_status", "status"),
        ("commit_id", "commit"),
        ("fixed", "fixed"),
        ("left", "left"),
    ] {
        agent.insert(target.to_owned(), value[source].clone());
    }
}

fn flatten(evidence: &mut Evidence, prefix: &str, value: &Value) {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                flatten(evidence, &format!("{prefix}.{key}"), value);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                flatten(evidence, &format!("{prefix}.{index}"), value);
            }
        }
        _ => evidence.push(
            "metrics",
            row(&[
                ("key", json!(prefix)),
                ("value", value.clone()),
                ("unit", json!(metric_unit(prefix))),
            ]),
        ),
    }
}

fn flatten_numbers(evidence: &mut Evidence, prefix: &str, value: &Value) {
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                flatten_numbers(evidence, &format!("{prefix}.{key}"), value);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                flatten_numbers(evidence, &format!("{prefix}.{index}"), value);
            }
        }
        Value::Number(_) | Value::Bool(_) => flatten(evidence, prefix, value),
        _ => {}
    }
}

fn metric_unit(key: &str) -> Option<&'static str> {
    [
        ("_ms", "ms"),
        ("_us", "us"),
        ("_s", "s"),
        ("_bytes", "bytes"),
        ("_kib", "KiB"),
        ("_mib", "MiB"),
    ]
    .into_iter()
    .find_map(|(suffix, unit)| key.ends_with(suffix).then_some(unit))
}

fn status(evidence: &mut Evidence, text: &str) -> Result<()> {
    let begin = Regex::new(r"integration begin (\S+) at ([0-9a-f]+)")?;
    let merge = Regex::new(r"integration (?:loop )?merged (\S+) at ([0-9a-f]+)")?;
    let agent = Regex::new(
        r"^(\S+) (\S+) (writer|breaker|judge|reviewer|fixer) (\S+) (\S+) exit (-?\d+) in (\d+) min",
    )?;
    let mut tip = String::new();
    for line in text.lines() {
        if let Some(c) = begin.captures(line) {
            tip = c[2].to_owned();
        }
        if let Some(c) = merge.captures(line) {
            evidence.push(
                "merges",
                row(&[
                    ("tip", json!(&c[2])),
                    ("fixer", json!(&c[1])),
                    ("parent_tip", json!(tip)),
                    ("conflict", json!(false)),
                    ("message", json!(line)),
                    ("timestamp", json!(line.split_whitespace().next())),
                ]),
            );
            tip = c[2].to_owned();
        } else if line.contains("conflict")
            || line.contains("fixer reply")
            || line.contains("merged")
            || begin.is_match(line)
        {
            evidence.push(
                "merges",
                row(&[
                    ("tip", json!(tip)),
                    ("conflict", json!(line.contains("conflict"))),
                    ("message", json!(line)),
                    ("timestamp", json!(line.split_whitespace().next())),
                ]),
            );
        }
        if let Some(c) = agent.captures(line) {
            evidence.push(
                "agents",
                row(&[
                    ("rid", json!(&c[2])),
                    ("name", json!(&c[4])),
                    ("model", json!(&c[5])),
                    ("kind", json!(&c[3])),
                    ("exit_code", json!(c[6].parse::<i64>().ok())),
                    ("minutes", json!(c[7].parse::<u64>().ok())),
                ]),
            );
        }
    }
    Ok(())
}

/// Get the latest integration tip from the ordered status stream.
pub fn integration_facts(text: &str) -> Result<Row> {
    let begin = Regex::new(r"integration begin (\S+) at ([0-9a-f]+)")?;
    Ok(begin.captures_iter(text).last().map_or_else(Row::new, |c| {
        row(&[("branch", json!(&c[1])), ("commit_id", json!(&c[2]))])
    }))
}

/// Read the exact summary lines emitted by integrate_build.py.
pub fn summary_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

/// Recover individual completed harness steps from the append-only status stream.
pub fn status_runs(path: &Path, text: &str) -> Result<Vec<Evidence>> {
    let begin = Regex::new(r"integration begin (\S+) at ([0-9a-f]+)")?;
    let stage = Regex::new(
        r"^(\S+) integration (build|tests|clippy-prod|clippy-all|clippy-nodef|lint) exit (-?\d+), (\d+) units, (\d+) failed tests",
    )?;
    let agent = Regex::new(
        r"^(\S+) (\S+) (writer|breaker|judge|reviewer|fixer) (\S+) (\S+) exit (-?\d+) in (\d+) min",
    )?;
    let mut facts = Row::new();
    let mut result = Vec::new();
    for (ordinal, line) in text.lines().enumerate() {
        if let Some(c) = begin.captures(line) {
            facts = row(&[("branch", json!(&c[1])), ("commit_id", json!(&c[2]))]);
        }
        let mut evidence = if let Some(c) = stage.captures(line) {
            let kind = if &c[2] == "tests" { "tier1" } else { &c[2] };
            let mut evidence = Evidence::new(&format!("{}|step|{ordinal}", path.display()), kind);
            evidence.run.extend(facts.clone());
            evidence
                .run
                .insert("exit_code".to_owned(), json!(c[3].parse::<i64>().ok()));
            evidence
                .run
                .insert("driver".to_owned(), json!("integrate_build.py"));
            evidence.push(
                "metrics",
                row(&[("key", json!("compile.units")), ("value", json!(&c[4]))]),
            );
            evidence.push(
                "metrics",
                row(&[("key", json!("tests.failed")), ("value", json!(&c[5]))]),
            );
            evidence
        } else if let Some(c) = agent.captures(line) {
            let mut evidence =
                Evidence::new(&format!("{}|step|{ordinal}", path.display()), "fixer");
            if &c[2] == "integration" {
                evidence.run.extend(facts.clone());
            }
            evidence
                .run
                .insert("exit_code".to_owned(), json!(c[6].parse::<i64>().ok()));
            evidence.run.insert("driver".to_owned(), json!(&c[4]));
            evidence.run.insert(
                "metadata".to_owned(),
                json!({"rid":&c[2],"model":&c[5],"kind":&c[3],"minutes":c[7].parse::<u64>().ok()}),
            );
            evidence
        } else {
            continue;
        };
        evidence
            .run
            .insert("log_path".to_owned(), json!(path.to_string_lossy()));
        evidence
            .run
            .insert("ended".to_owned(), json!(line.split_whitespace().next()));
        evidence.push(
            "evidence_lines",
            row(&[("ordinal", json!(ordinal + 1)), ("text", json!(line))]),
        );
        result.push(evidence);
    }
    Ok(result)
}

/// Restrict integration-file attribution to stages observed after the latest begin.
pub fn current_stage_facts(text: &str, filename: &str) -> Result<Row> {
    let facts = integration_facts(text)?;
    if filename == "summary.txt" {
        return Ok(facts);
    }
    let stage = filename.strip_suffix(".log").unwrap_or(filename);
    let tail = text.rsplit("integration begin ").next().unwrap_or("");
    let pattern = Regex::new(&format!(
        r"integration {} exit (-?\d+)",
        regex::escape(stage)
    ))?;
    if let Some(c) = pattern.captures_iter(tail).last() {
        let mut facts = facts;
        facts.insert("exit_code".to_owned(), json!(c[1].parse::<i64>().ok()));
        return Ok(facts);
    }
    Ok(Row::new())
}

/// Join usage/replies with the matching status facts for the latest named attempt.
pub fn agent_facts(text: &str, rid: &str, name: &str) -> Result<Row> {
    let pattern = Regex::new(&format!(
        r"(?m)^\S+ {} (writer|breaker|judge|reviewer|fixer) {} (\S+) exit (-?\d+) in (\d+) min",
        regex::escape(rid),
        regex::escape(name)
    ))?;
    let retry = Regex::new(&format!(
        r"(?m)^\S+ {} .*? {} .*?at capacity, retry (\d+)",
        regex::escape(rid),
        regex::escape(name)
    ))?;
    let mut facts = pattern
        .captures_iter(text)
        .last()
        .map_or_else(Row::new, |c| {
            row(&[
                ("kind", json!(&c[1])),
                ("model", json!(&c[2])),
                ("exit_code", json!(c[3].parse::<i64>().ok())),
                ("minutes", json!(c[4].parse::<u64>().ok())),
            ])
        });
    if let Some(c) = retry.captures_iter(text).last() {
        facts.insert("retries".to_owned(), json!(c[1].parse::<u64>().ok()));
    }
    Ok(facts)
}
