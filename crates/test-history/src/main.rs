//! Harness CLI and stress workers using the test-history backend contract.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use soe_test_history::store::{HistoryStore, Mode, TursoStore};
use soe_test_history::{parse, row, Error, Evidence, Result, Row, LOG_ROOT, SCHEMA};

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build();
    let result = runtime
        .map_err(Error::from)
        .and_then(|runtime| runtime.block_on(run()));
    if let Err(error) = result {
        eprintln!("soe-test-history: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args
        .first()
        .cloned()
        .ok_or_else(|| Error::Input(usage().to_owned()))?;
    args.remove(0);
    if command == "schema" {
        print!("{SCHEMA}");
        return Ok(());
    }
    let path = option(&mut args, "--db")
        .or_else(|| std::env::var("SOE_TEST_HISTORY").ok())
        .ok_or_else(|| Error::Input("set SOE_TEST_HISTORY or pass --db PATH".to_owned()))?;
    let mode = Mode::parse(
        &option(&mut args, "--mode")
            .or_else(|| std::env::var("SOE_TEST_HISTORY_MODE").ok())
            .unwrap_or_else(|| "multiprocess".to_owned()),
    )?;
    let backend = TursoStore::open(Path::new(&path), mode, command != "stress-worker").await?;
    let store: &dyn HistoryStore = &backend;
    match command.as_str() {
        "ingest" | "rebuild" => {
            let root = option(&mut args, "--root").unwrap_or_else(|| LOG_ROOT.to_owned());
            if command == "rebuild" {
                let mut archive = Path::new(&path).as_os_str().to_owned();
                archive.push(".records");
                let archive = PathBuf::from(archive);
                if archive.is_dir() {
                    args.push(archive.to_string_lossy().into_owned());
                }
                let known = store
                    .sql("SELECT log_path,record_path FROM runs WHERE log_path IS NOT NULL OR record_path IS NOT NULL", &[])
                    .await?;
                args.extend(
                    known
                        .iter()
                        .flat_map(|r| {
                            [r.get("log_path"), r.get("record_path")]
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                        })
                        .filter_map(|path| {
                            let path = PathBuf::from(path);
                            if path.is_file() {
                                Some(path.to_string_lossy().into_owned())
                            } else {
                                let pending = path.with_extension("pending");
                                pending
                                    .is_file()
                                    .then(|| pending.to_string_lossy().into_owned())
                            }
                        }),
                );
                for directory in ["log", "lead-evidence", "lead-gpu-logs", "play"] {
                    let path = Path::new(&root).join(directory);
                    if path.exists() {
                        args.push(path.to_string_lossy().into_owned());
                    }
                }
                for name in ["status.log", "summary.json"] {
                    let path = Path::new(&root).join(name);
                    if path.exists() {
                        args.push(path.to_string_lossy().into_owned());
                    }
                }
            }
            let mut files = parse::discover(&args)?;
            files.sort_by_key(|path| {
                (
                    path.components()
                        .any(|c| c.as_os_str().to_string_lossy().ends_with(".records")),
                    path.clone(),
                )
            });
            let ids = parse::backlog(Path::new("BACKLOG.csv"))?;
            if command == "rebuild" {
                store.clear().await?;
            }
            let count = ingest(store, &files, &ids, Path::new(&root)).await?;
            println!("{}", json!({"ingested_runs":count}));
        }
        "record" => record(store, &mut args, Path::new(&path)).await?,
        "query" => {
            let format = option(&mut args, "--format").unwrap_or_else(|| "table".to_owned());
            let name = args
                .first()
                .ok_or_else(|| Error::Input(usage().to_owned()))?;
            let rows = store.query(name, &args[1..]).await?;
            output(&rows, &format)?;
        }
        "sql" => {
            let sql = args
                .first()
                .ok_or_else(|| Error::Input("sql requires one SQL argument".to_owned()))?;
            if ![
                "select",
                "pragma integrity_check",
                "pragma foreign_key_check",
            ]
            .iter()
            .any(|prefix| sql.to_ascii_lowercase().trim_start().starts_with(prefix))
            {
                return Err(Error::Input("sql accepts read queries only".to_owned()));
            }
            output(&store.sql(sql, &args[1..]).await?, "json")?;
        }
        "stress-worker" => stress_worker(store, &args).await?,
        _ => return Err(Error::Input(usage().to_owned())),
    }
    Ok(())
}

fn usage() -> &'static str {
    "soe-test-history {ingest PATH...|rebuild|record FILE|-|--log PATH --kind KIND --exit CODE|query NAME [ARGS] [--format table|json]|schema} [--db PATH] [--mode locked|multiprocess|mvcc]"
}

fn option(args: &mut Vec<String>, key: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == key)?;
    if index + 1 == args.len() {
        return None;
    }
    args.remove(index);
    Some(args.remove(index))
}

async fn record(store: &dyn HistoryStore, args: &mut Vec<String>, database: &Path) -> Result<()> {
    let log = option(args, "--log");
    let explicit_id = args.iter().any(|arg| arg == "--id");
    let mut evidence = if let Some(path) = &log {
        parse::file(
            Path::new(path),
            &parse::backlog(Path::new("BACKLOG.csv"))?,
            &Row::new(),
        )?
    } else {
        let path = args
            .first()
            .ok_or_else(|| Error::Input(usage().to_owned()))?;
        let mut text = String::new();
        if path == "-" {
            std::io::stdin().read_to_string(&mut text)?;
        } else {
            text = std::fs::read_to_string(path)?;
        }
        serde_json::from_str::<Evidence>(&text)?
    };
    if log.is_some() && !explicit_id {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Input(e.to_string()))?
            .as_nanos();
        evidence.run.insert(
            "id".to_owned(),
            json!(format!(
                "{}|record|{now}|{}",
                evidence.id()?,
                std::process::id()
            )),
        );
    }
    for (flag, column) in [
        ("--id", "id"),
        ("--kind", "kind"),
        ("--commit", "commit_id"),
        ("--parents", "parents"),
        ("--branch", "branch"),
        ("--started", "started"),
        ("--ended", "ended"),
        ("--driver", "driver"),
    ] {
        if let Some(value) = option(args, flag) {
            let value = if column == "parents" {
                serde_json::from_str::<Value>(&value)
                    .unwrap_or_else(|_| json!(value.split_whitespace().collect::<Vec<_>>()))
            } else {
                json!(value)
            };
            evidence.run.insert(column.to_owned(), value);
        }
    }
    for (flag, column) in [("--exit", "exit_code"), ("--attempt", "attempt")] {
        if let Some(value) = option(args, flag) {
            evidence.run.insert(
                column.to_owned(),
                json!(value
                    .parse::<i64>()
                    .map_err(|e| Error::Input(e.to_string()))?),
            );
        }
    }
    current_facts(&mut evidence.run)?;
    evidence.omit_image_bytes()?;
    if let Some(path) = log {
        soe_test_history::write_manifest(Path::new(&path), &evidence)?;
        evidence = parse::file(
            Path::new(&path),
            &parse::backlog(Path::new("BACKLOG.csv"))?,
            &evidence.run,
        )?;
    }
    let mut directory = database.as_os_str().to_owned();
    directory.push(".records");
    let directory = PathBuf::from(directory);
    std::fs::create_dir_all(&directory)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Error::Input(e.to_string()))?
        .as_nanos();
    let archive = directory.join(format!("{timestamp}-{}.json", std::process::id()));
    evidence
        .run
        .insert("record_path".to_owned(), json!(archive.to_string_lossy()));
    let pending = archive.with_extension("pending");
    std::fs::write(&pending, serde_json::to_vec(&evidence)?)?;
    let stats = store.record(&evidence).await?;
    std::fs::rename(&pending, &archive)?;
    println!("{}", json!({"run":evidence.id()?,"write":stats}));
    Ok(())
}

fn current_facts(facts: &mut Row) -> Result<()> {
    if !facts.contains_key("commit_id") {
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
        {
            if output.status.success() {
                facts.insert(
                    "commit_id".to_owned(),
                    json!(String::from_utf8_lossy(&output.stdout).trim()),
                );
            }
        }
    }
    if !facts.contains_key("branch") {
        if let Ok(output) = std::process::Command::new("git")
            .args(["symbolic-ref", "--short", "-q", "HEAD"])
            .output()
        {
            if output.status.success() {
                facts.insert(
                    "branch".to_owned(),
                    json!(String::from_utf8_lossy(&output.stdout).trim()),
                );
            }
        }
    }
    add_parents(facts);
    facts.entry("ended".to_owned()).or_insert_with(|| {
        json!(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
            .to_string())
    });
    facts.entry("cores".to_owned()).or_insert_with(|| {
        json!(std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get))
    });
    for (variable, key) in [
        ("SKYRIM_DATA", "skyrim_data"),
        ("SOE_TEST_SAVE", "test_save"),
        ("SOE_TEST_SKYUI", "test_skyui"),
        ("SOE_PLAY_PATH", "display_mode"),
    ] {
        if let Ok(value) = std::env::var(variable) {
            facts.entry(key.to_owned()).or_insert(json!(value));
        }
    }
    let display = ["DISPLAY", "WAYLAND_DISPLAY", "SOE_PLAY_PATH"]
        .into_iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| (key.to_owned(), json!(value)))
        })
        .collect::<Row>();
    if !display.is_empty() {
        facts
            .entry("display_mode".to_owned())
            .or_insert(json!(display));
    }
    let mem = std::fs::read_to_string("/proc/meminfo")?;
    if let Some(kib) = mem.lines().find_map(|line| {
        line.strip_prefix("MemTotal:")
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse::<u64>().ok())
    }) {
        facts
            .entry("ram_bytes".to_owned())
            .or_insert(json!(kib * 1024));
    }
    for (key, path) in [
        ("nvme_free_bytes", "/run/host/media/pengolodh/Data"),
        ("games_free_bytes", "/run/media/pengolodh/linux-data-games"),
    ] {
        if let Ok(output) = std::process::Command::new("df")
            .args(["-B1", "--output=avail", path])
            .output()
        {
            if let Some(bytes) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .last()
                .and_then(|s| s.trim().parse::<u64>().ok())
            {
                facts.entry(key.to_owned()).or_insert(json!(bytes));
            }
        }
    }
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if output.status.success() {
            facts
                .entry("gpu".to_owned())
                .or_insert(json!(String::from_utf8_lossy(&output.stdout).trim()));
        }
    }
    Ok(())
}

async fn ingest(
    store: &dyn HistoryStore,
    files: &[PathBuf],
    ids: &BTreeSet<String>,
    root: &Path,
) -> Result<usize> {
    let status = std::fs::read_to_string(root.join("status.log")).unwrap_or_default();
    let integration = std::fs::read(root.join("integration.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let mut count = 0;
    for path in files {
        let mut context = Row::new();
        if let Some(parent) = path.parent() {
            if let Ok(head) = std::fs::read_to_string(parent.join("head.txt")) {
                let head = head.trim();
                if head.len() >= 7
                    && head.len() <= 40
                    && head.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    context.insert("commit_id".to_owned(), json!(head));
                }
            }
        }
        if path.parent() == Some(root.join("log/integration").as_path()) {
            context = parse::current_stage_facts(
                &status,
                path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            )?;
            if let (Some(commit), Some(full)) = (
                context.get("commit_id").and_then(Value::as_str),
                integration.as_ref().and_then(|v| v["tip"].as_str()),
            ) {
                if full.starts_with(commit) {
                    context.insert("commit_id".to_owned(), json!(full));
                }
            }
        }
        let mut evidence = parse::file(path, ids, &context)?;
        add_parents(&mut evidence.run);
        if let Some(merges) = evidence.rows.get_mut("merges") {
            let mut expanded = Vec::new();
            for merge in merges.iter() {
                let parents = merge
                    .get("tip")
                    .and_then(Value::as_str)
                    .and_then(git_parents)
                    .unwrap_or_default();
                if parents.len() > 1 {
                    for head in parents.iter().skip(1) {
                        let mut value = merge.clone();
                        value.insert("parent_tip".to_owned(), json!(parents[0]));
                        value.insert("head".to_owned(), json!(head));
                        expanded.push(value);
                    }
                } else {
                    expanded.push(merge.clone());
                }
            }
            *merges = expanded;
        }
        if path.file_name().is_some_and(|s| s == "status.log") {
            if let Some(agents) = evidence.rows.get_mut("agents") {
                let mut seen = BTreeSet::new();
                agents.reverse();
                agents.retain(|agent| {
                    let rid = agent.get("rid").and_then(Value::as_str).unwrap_or("");
                    let name = agent.get("name").and_then(Value::as_str).unwrap_or("");
                    let latest = seen.insert((rid.to_owned(), name.to_owned()));
                    !latest
                        || !root
                            .join("log")
                            .join(rid)
                            .join(format!("{name}.events.jsonl"))
                            .is_file()
                });
                agents.reverse();
            }
        }
        if let Some(agents) = evidence.rows.get_mut("agents") {
            for agent in agents {
                let rid = agent.get("rid").and_then(Value::as_str).unwrap_or("");
                let name = agent.get("name").and_then(Value::as_str).unwrap_or("");
                agent.extend(parse::agent_facts(&status, rid, name)?);
            }
        }
        store.record(&evidence).await?;
        count += 1;
        if std::env::var_os("SOE_TEST_HISTORY_PROGRESS").is_some() {
            eprintln!("ingested {count}: {}", path.display());
        }
        if path.file_name().is_some_and(|s| s == "status.log") {
            for mut evidence in parse::status_runs(path, &status)? {
                add_parents(&mut evidence.run);
                store.record(&evidence).await?;
                count += 1;
            }
        }
    }
    Ok(count)
}

fn add_parents(facts: &mut Row) {
    if !facts.contains_key("parents") {
        if let Some(parents) = facts
            .get("commit_id")
            .and_then(Value::as_str)
            .and_then(git_parents)
        {
            facts.insert("parents".to_owned(), json!(parents));
        }
    }
}

fn git_parents(commit: &str) -> Option<Vec<String>> {
    if !(7..=64).contains(&commit.len()) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let output = std::process::Command::new("git")
        .args(["show", "-s", "--format=%P", commit, "--"])
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    })
}

fn output(rows: &[Row], format: &str) -> Result<()> {
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(rows)?),
        "table" => {
            if let Some(first) = rows.first() {
                println!("{}", first.keys().cloned().collect::<Vec<_>>().join("\t"));
                for row in rows {
                    println!(
                        "{}",
                        row.values()
                            .map(|v| v
                                .as_str()
                                .map_or_else(|| v.to_string(), str::to_owned)
                                .replace(['\n', '\t'], " "))
                            .collect::<Vec<_>>()
                            .join("\t")
                    );
                }
            }
        }
        _ => return Err(Error::Input("format must be table or json".to_owned())),
    }
    Ok(())
}

async fn stress_worker(store: &dyn HistoryStore, args: &[String]) -> Result<()> {
    if args.len() != 4 {
        return Err(Error::Input(
            "stress-worker ROLE ID SECONDS DIRECTORY".to_owned(),
        ));
    }
    let role = &args[0];
    let worker = &args[1];
    let seconds = args[2]
        .parse::<u64>()
        .map_err(|e| Error::Input(e.to_string()))?;
    let directory = Path::new(&args[3]);
    std::fs::write(directory.join(format!("{role}-{worker}.ready")), b"ready")?;
    while !directory.join("start").exists() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let start = Instant::now();
    let mut sequence = 0u64;
    let mut ledger = std::fs::File::create(directory.join(format!("{role}-{worker}.jsonl")))?;
    while start.elapsed() < Duration::from_secs(seconds) {
        if role == "writer" {
            let id = format!("stress-{worker}-{sequence}");
            let mut evidence = Evidence::new(&id, "pass");
            evidence.run.insert("commit_id".to_owned(), json!("stress"));
            evidence.run.insert("parents".to_owned(), json!([]));
            evidence
                .run
                .insert("started".to_owned(), json!(sequence.to_string()));
            evidence.push(
                "tests",
                row(&[
                    ("package", json!("soe-test-history")),
                    ("binary", json!(worker)),
                    ("name", json!(id)),
                    (
                        "status",
                        json!(if sequence.is_multiple_of(2) {
                            "pass"
                        } else {
                            "fail"
                        }),
                    ),
                ]),
            );
            evidence.push(
                "metrics",
                row(&[("key", json!("sequence")), ("value", json!(sequence))]),
            );
            let stats = store.record(&evidence).await?;
            writeln!(
                ledger,
                "{}",
                json!({"id":id,"sequence":sequence,"write":stats})
            )?;
            ledger.flush()?;
        } else {
            let name = soe_test_history::queries::NAMES
                [sequence as usize % soe_test_history::queries::NAMES.len()];
            let args = if ["regressions", "new-failures", "summary"].contains(&name) {
                vec!["stress".to_owned()]
            } else {
                vec![]
            };
            store.query(name, &args).await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        sequence += 1;
    }
    writeln!(
        ledger,
        "{}",
        json!({"finished":true,"operations":sequence,"seconds":start.elapsed().as_secs_f64()})
    )?;
    Ok(())
}
