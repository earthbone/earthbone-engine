// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! TST-19 runs the real walkthrough and validates its complete observation log.
//! An independent Rust thread watches main-thread heartbeats. Godot's local
//! thread-safe API and RenderingServer documentation define the host boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const CHECKS: &[&str] = &[
    "window",
    "capture",
    "route.walk",
    "route.hulda",
    "route.container",
    "route.loose",
    "route.hearth",
    "route.exit",
    "route.market",
    "route.gate",
    "route.return",
    "route.repeat",
    "duration",
    "npc.drawn",
    "npc.head_hair",
    "npc.pose",
    "frame.max_ms",
    "watchdog.max_ms",
    "fire.additive",
    "fire.hull",
    "terrain.textures",
    "object.white",
    "object.height",
    "doors.drawn",
    "doors.states",
    "compass.offset",
    "menu.inventory_theme",
    "menu.container_theme",
    "menu.dialogue_theme",
    "audio.stone",
    "audio.grass",
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn theme_cache(output: &Path, data: &Path) -> Result<Option<PathBuf>, String> {
    let skyui = std::env::var_os("SOE_TEST_SKYUI");
    let nordic = std::env::var_os("SOE_TEST_NORDIC");
    if skyui.is_none() && nordic.is_none() {
        return Ok(None);
    }
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_archive_path(&data.join("Skyrim - Interface.bsa"))
        .map_err(|e| e.to_string())?;
    if let Some(skyui) = skyui {
        let directory = PathBuf::from(skyui);
        vfs.add_archive_path(&directory.join("SkyUI_SE.bsa"))
            .map_err(|e| e.to_string())?;
        vfs.add_loose_root(&directory).map_err(|e| e.to_string())?;
    }
    if let Some(nordic) = nordic {
        vfs.add_loose_root(Path::new(&nordic))
            .map_err(|e| e.to_string())?;
    }
    let directory = output.join("converted-interface");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut movies = serde_json::Map::new();
    for name in [
        "icons_item_psychosteve",
        "icons_category_psychosteve",
        "inventorylists",
        "itemcard",
    ] {
        let path = format!("interface/skyui/{name}.swf");
        let bytes = vfs.read(&path).map_err(|e| format!("{path}: {e}"))?;
        let movie = soe_swf_convert::convert(&bytes).map_err(|e| format!("{path}: {e}"))?;
        let destination = directory.join(name);
        soe_swf_convert::write_art(&movie, &destination).map_err(|e| e.to_string())?;
        fs::write(
            destination.join("movie.json"),
            serde_json::to_vec(&movie).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        movies.insert(path, format!("{name}/movie.tres").into());
    }
    fs::write(
        directory.join("manifest.json"),
        serde_json::to_vec(&json!({"movies": movies})).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(directory))
}

fn statuses(backlog: &str) -> BTreeMap<String, String> {
    backlog
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.trim_matches('"').split("\",\"").collect();
            Some((fields.first()?.to_string(), fields.get(3)?.to_string()))
        })
        .collect()
}

fn policy_for(baseline: &Value, backlog: &str) -> Value {
    let statuses = statuses(backlog);
    let entries = baseline.as_object().expect("expected-failure table");
    let mut result = serde_json::Map::new();
    for (check, entry) in entries {
        assert!(
            CHECKS.contains(&check.as_str()),
            "unknown expected check {check}"
        );
        let owner = entry["owner"].as_str().expect("failure owner");
        let (area, number) = owner.split_once('-').expect("owner row ID");
        assert!(
            matches!(area, "HST" | "RND" | "UI" | "AUD" | "PRF")
                && number.len() == 2
                && number.bytes().all(|byte| byte.is_ascii_digit()),
            "invalid play path owner {owner}"
        );
        assert!(
            entry["evidence"].as_str().is_some_and(|s| !s.is_empty()),
            "missing baseline for {check}"
        );
        let required = statuses.get(owner).is_some_and(|status| status == "done");
        result.insert(
            check.clone(),
            json!({"owner": owner, "expected": !required}),
        );
    }
    Value::Object(result)
}

const STOP_CHECKS: &[&str] = &[
    "npc.drawn",
    "npc.head_hair",
    "doors.drawn",
    "frame.max_ms",
    "watchdog.max_ms",
];
const EXTERIOR_CHECKS: &[&str] = &["terrain.textures", "object.white", "object.height"];

fn validate(stdout: &str, policy: &Value, duration: f64) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut stops = BTreeSet::new();
    let mut at_stop = BTreeSet::new();
    let mut errors = Vec::new();
    let mut summary = None;
    for line in stdout.lines() {
        if let Some(payload) = line.strip_prefix("PLAY_PATH_RESULT ") {
            summary = Some(serde_json::from_str::<Value>(payload).map_err(|e| e.to_string())?);
        } else if let Some(payload) = line.strip_prefix("PLAY_PATH ") {
            let row: Value = serde_json::from_str(payload).map_err(|e| e.to_string())?;
            let check = row["check"].as_str().ok_or("missing check id")?;
            let stop = row["stop"].as_str().ok_or("missing route stop")?;
            let passed = row["pass"].as_bool().ok_or("missing pass flag")?;
            if !row["value"].is_object()
                || row["value"]
                    .as_object()
                    .is_none_or(serde_json::Map::is_empty)
            {
                return Err(format!("{check} has no measured value"));
            }
            seen.insert(check.to_owned());
            stops.insert(stop.to_owned());
            at_stop.insert((stop.to_owned(), check.to_owned()));
            let expected = policy[check]["expected"].as_bool().unwrap_or(false);
            if !passed
                && (!expected || row["owner"] != policy[check]["owner"] || row["status"] != "XFAIL")
            {
                errors.push(format!("{check} at {stop}: {}", row["value"]));
            }
            if passed && row["status"] != "PASS" {
                return Err(format!("{check}: inconsistent verdict"));
            }
        }
    }
    for check in CHECKS {
        if !seen.contains(*check) {
            errors.push(format!("missing {check}"));
        }
    }
    for stop in [
        "hulda",
        "container",
        "loose",
        "hearth",
        "exit",
        "market",
        "gate",
        "return",
    ] {
        if !stops.contains(stop) {
            errors.push(format!("unvisited stop {stop}"));
        }
        for check in STOP_CHECKS
            .iter()
            .chain(if matches!(stop, "market" | "gate" | "return") {
                EXTERIOR_CHECKS
            } else {
                &[]
            })
        {
            if !at_stop.contains(&(stop.to_owned(), (*check).to_owned())) {
                errors.push(format!("missing {check} at {stop}"));
            }
        }
    }
    if let Some(summary) = summary {
        if summary["elapsed_s"]
            .as_f64()
            .is_none_or(|elapsed| elapsed < duration)
            || summary["loops"].as_u64().is_none_or(|loops| loops == 0)
            || summary["required_failures"] != 0
        {
            errors.push(format!("incomplete route: {summary}"));
        }
    } else {
        errors.push("missing completion summary".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

struct OwnedProcess(Child);

fn spawn_owned(command: &mut Command) -> OwnedProcess {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    OwnedProcess(command.spawn().expect("start tracked process"))
}

fn terminate(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--"])
            .arg(format!("-{}", child.id()))
            .status();
    }
    let _ = child.kill();
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            terminate(&mut self.0);
        }
        let _ = self.0.wait();
    }
}

fn display_ready() -> bool {
    Command::new("timeout")
        .args(["1s", "xprop", "-display", ":99", "-root", "WM_NAME"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn gate_display() -> Option<OwnedProcess> {
    if display_ready() {
        return None;
    }
    let mut display = spawn_owned(
        Command::new("Xvfb")
            .args([
                ":99",
                "-screen",
                "0",
                "640x360x24",
                "-nolisten",
                "tcp",
                "-ac",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    );
    let start = Instant::now();
    loop {
        if display_ready() {
            return Some(display);
        }
        assert!(
            display.0.try_wait().unwrap().is_none(),
            "Xvfb :99 exited before connecting"
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "Xvfb :99 did not become ready"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn heartbeat(path: &Path) -> Option<(u64, bool, String)> {
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some((
        value["ticks"].as_u64()?,
        value["armed"].as_bool()?,
        value["stop"].as_str()?.to_owned(),
    ))
}

fn watch(
    child: &mut Child,
    heartbeat_path: &Path,
    output: &Path,
    stall: Duration,
    total: Duration,
) -> (ExitStatus, bool) {
    let stop = AtomicBool::new(false);
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        let monitor = scope.spawn(|| {
            let started = Instant::now();
            let mut changed = started;
            let mut previous = None;
            let mut armed = false;
            let mut stage = String::from("startup");
            while !stop.load(Ordering::Acquire) {
                if let Some((ticks, active, current_stage)) = heartbeat(heartbeat_path) {
                    if previous != Some(ticks) {
                        changed = Instant::now();
                        previous = Some(ticks);
                    }
                    armed = active;
                    stage = current_stage;
                }
                if (armed && changed.elapsed() > stall) || started.elapsed() > total {
                    let _ = sender.send((stage, changed.elapsed()));
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let result = loop {
            if let Some(status) = child.try_wait().expect("poll tracked Godot process") {
                break (status, false);
            }
            if let Ok((stage, elapsed)) = receiver.try_recv() {
                let mut file = File::options()
                    .create(true)
                    .append(true)
                    .open(output)
                    .unwrap();
                writeln!(
                    file,
                    "PLAY_PATH_BACKTRACE pid={} stop={stage} stale_ms={}",
                    child.id(),
                    elapsed.as_millis()
                )
                .unwrap();
                let trace = Command::new("timeout")
                    .args([
                        "1s",
                        "gdb",
                        "--batch",
                        "-ex",
                        "set pagination off",
                        "-ex",
                        "thread apply all bt",
                        "-p",
                    ])
                    .arg(child.id().to_string())
                    .output();
                match trace {
                    Ok(trace) => {
                        file.write_all(&trace.stdout).unwrap();
                        file.write_all(&trace.stderr).unwrap();
                    }
                    Err(error) => writeln!(file, "PLAY_PATH_BACKTRACE debugger: {error}").unwrap(),
                }
                writeln!(file, "PLAY_PATH {}", json!({"check": "watchdog.max_ms", "status": "FAIL", "pass": false, "owner": "PRF-38", "stop": stage, "value": {"stale_ms": elapsed.as_millis(), "limit_ms": 2000}})).unwrap();
                terminate(child);
                break (child.wait().expect("collect killed Godot exit"), true);
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        stop.store(true, Ordering::Release);
        monitor.join().expect("watchdog thread joined");
        result
    })
}

#[test]
fn tst_19_expected_failures_become_required_when_owner_is_done() {
    let baseline = json!({"npc.drawn": {"owner": "HST-23", "evidence": "installed: zero character instances"}});
    for status in ["open", "wip", "blocked", "done"] {
        let backlog = format!("\"HST-23\",\"HST\",\"Draw NPCs\",\"{status}\"\n");
        assert_eq!(
            policy_for(&baseline, &backlog)["npc.drawn"]["expected"],
            status != "done"
        );
    }
}

#[test]
fn tst_19_incomplete_routes_and_missing_observations_fail() {
    let result = validate(
        "PLAY_PATH_RESULT {\"elapsed_s\":180,\"loops\":1,\"required_failures\":0}",
        &json!({}),
        180.0,
    )
    .unwrap_err();
    for check in CHECKS {
        assert!(result.contains(&format!("missing {check}")), "{result}");
    }
    assert!(validate("", &json!({}), 180.0)
        .unwrap_err()
        .contains("missing completion"));
}

#[test]
fn tst_19_unowned_failures_and_false_expected_verdicts_fail() {
    for owner in ["TST-19", "UI-abc", "NOT-A-ROW"] {
        let baseline = json!({"npc.drawn": {"owner": owner, "evidence": "invalid owner fixture"}});
        assert!(std::panic::catch_unwind(|| policy_for(&baseline, "")).is_err());
    }
    let row = "PLAY_PATH {\"check\":\"npc.drawn\",\"stop\":\"hulda\",\"status\":\"XFAIL\",\"pass\":false,\"owner\":\"HST-23\",\"value\":{\"drawn\":0}}";
    assert!(validate(row, &json!({}), 180.0)
        .unwrap_err()
        .contains("npc.drawn at hulda"));
    let policy = json!({"npc.drawn": {"expected": true, "owner": "HST-25"}});
    assert!(validate(row, &policy, 180.0)
        .unwrap_err()
        .contains("npc.drawn at hulda"));
}

#[test]
fn tst_19_every_required_check_and_full_duration_are_enforced() {
    let stops = [
        "hulda",
        "container",
        "loose",
        "hearth",
        "exit",
        "market",
        "gate",
        "return",
    ];
    let mut rows: Vec<_> = CHECKS
        .iter()
        .enumerate()
        .map(|(index, check)| {
            json!({"check": check, "stop": stops[index % stops.len()], "status": "PASS",
            "pass": true, "owner": "TST-19", "value": {"sample": 1}})
        })
        .collect();
    for stop in stops {
        for check in STOP_CHECKS
            .iter()
            .chain(if matches!(stop, "market" | "gate" | "return") {
                EXTERIOR_CHECKS
            } else {
                &[]
            })
        {
            rows.push(
                json!({"check": check, "stop": stop, "status": "PASS", "pass": true,
                "owner": "TST-19", "value": {"sample": 1}}),
            );
        }
    }
    let report = |rows: &[Value], seconds: f64| {
        let mut output = String::new();
        for row in rows {
            writeln!(output, "PLAY_PATH {row}").unwrap();
        }
        output.push_str(&format!(
            "PLAY_PATH_RESULT {}",
            json!({"elapsed_s": seconds, "loops": 1, "required_failures": 0})
        ));
        output
    };
    assert!(validate(&report(&rows, 180.0), &json!({}), 180.0).is_ok());
    assert!(validate(&report(&rows, 179.9), &json!({}), 180.0).is_err());
    assert!(validate(&report(&rows, 180.0), &json!({}), 600.0).is_err());
    let omitted: Vec<_> = rows
        .iter()
        .filter(|row| !(row["stop"] == "gate" && row["check"] == "object.white"))
        .cloned()
        .collect();
    assert!(validate(&report(&omitted, 180.0), &json!({}), 180.0)
        .unwrap_err()
        .contains("missing object.white at gate"));
    for (index, check) in CHECKS.iter().enumerate() {
        let mut rejected = rows.clone();
        rejected[index]["pass"] = false.into();
        rejected[index]["status"] = "FAIL".into();
        let error = validate(&report(&rejected, 180.0), &json!({}), 180.0).unwrap_err();
        assert!(error.contains(&format!("{check} at")), "{error}");
    }
}

#[test]
fn tst_19_watchdog_terminates_a_stalled_process_and_records_a_backtrace_marker() {
    #[cfg(unix)]
    {
        let temp = tempfile::tempdir_in(workspace()).unwrap();
        let heartbeat = temp.path().join("heartbeat.json");
        fs::write(
            &heartbeat,
            "{\"ticks\":1,\"armed\":true,\"stop\":\"forced_stall\"}",
        )
        .unwrap();
        let output = temp.path().join("watchdog.log");
        let mut child = spawn_owned(Command::new("sleep").arg("30"));
        let (status, timed_out) = watch(
            &mut child.0,
            &heartbeat,
            &output,
            Duration::from_millis(100),
            Duration::from_secs(5),
        );
        assert!(timed_out && !status.success());
        let log = fs::read_to_string(output).unwrap();
        assert!(log.contains("PLAY_PATH_BACKTRACE") && log.contains("forced_stall"));
        assert!(log.contains("\"check\":\"watchdog.max_ms\"") && log.contains("\"pass\":false"));
    }
}

#[test]
fn tst_19_pixel_and_pose_predicates_reject_blank_frozen_and_opaque_controls() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let directory = tempfile::tempdir_in(workspace()).unwrap();
    let stdout_path = directory.path().join("stdout.log");
    let stderr_path = directory.path().join("stderr.log");
    let mut command = Command::new(godot);
    command
        .args(["--headless", "--path"])
        .arg(workspace().join("godot"))
        .args(["--script", "tst_19_play_path.gd"])
        .env("SOE_PLAY_PATH_SELF_TEST", "1")
        .env("SOE_GODOT_TIMEOUT", "60")
        .stdout(File::create(&stdout_path).unwrap())
        .stderr(File::create(&stderr_path).unwrap());
    let mut child = spawn_owned(&mut command);
    let (status, timed_out) = watch(
        &mut child.0,
        &directory.path().join("heartbeat.json"),
        &stdout_path,
        Duration::from_secs(2),
        Duration::from_secs(60),
    );
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(
        !timed_out
            && status.success()
            && stdout.contains("tst_19_pixel_and_pose_predicates: PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{stdout}\n{stderr}"
    );
}

#[test]
fn tst_19_installed_whiterun_play_path_checks_every_stop_and_continuous_observation() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("Godot for installed play path");
    let mode = std::env::var("SOE_PLAY_PATH").unwrap_or_else(|_| "gate".into());
    assert!(
        matches!(mode.as_str(), "gate" | "gpu"),
        "invalid SOE_PLAY_PATH {mode}"
    );
    let duration = if mode == "gate" { 180.0 } else { 600.0 };
    let output_directory = workspace().join("target/tst_19").join(&mode);
    fs::create_dir_all(&output_directory).unwrap();
    let baseline: Value = serde_json::from_slice(
        &fs::read(workspace().join("godot/tst_19_expected_failures.json")).unwrap(),
    )
    .unwrap();
    let policy = policy_for(
        &baseline,
        &fs::read_to_string(workspace().join("BACKLOG.csv")).unwrap(),
    );
    let policy_path = output_directory.join("policy.json");
    fs::write(&policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();
    let heartbeat_path = output_directory.join("heartbeat.json");
    let _ = fs::remove_file(&heartbeat_path);
    let _display = (mode == "gate").then(gate_display);
    let stdout_path = output_directory.join("stdout.log");
    let stderr_path = output_directory.join("stderr.log");
    fs::write(&stdout_path, "").unwrap();
    let mut command = Command::new(godot);
    match theme_cache(&output_directory, Path::new(&data)) {
        Ok(Some(cache)) => {
            command.env("SOE_INTERFACE_CACHE", cache);
        }
        Ok(None) => {}
        Err(error) => {
            command.env("SOE_PLAY_PATH_THEME_ERROR", error);
        }
    }
    command
        .arg("--path")
        .arg(workspace().join("godot"))
        .args(["--resolution", "640x360", "--script", "tst_19_play_path.gd"])
        .env("SKYRIM_DATA", data)
        .env("SOE_PLAY_PATH", &mode)
        .env("SOE_GODOT_TIMEOUT", (duration as u64 + 180).to_string())
        .env("SOE_PLAY_PATH_OUTPUT", &output_directory)
        .env("SOE_PLAY_PATH_HEARTBEAT", &heartbeat_path)
        .env("SOE_PLAY_PATH_POLICY", &policy_path)
        .stdout(
            File::options()
                .create(true)
                .append(true)
                .open(&stdout_path)
                .unwrap(),
        )
        .stderr(File::create(&stderr_path).unwrap());
    if mode == "gate" {
        command.env("DISPLAY", ":99");
    }
    let mut child = spawn_owned(&mut command);
    let (status, stalled) = watch(
        &mut child.0,
        &heartbeat_path,
        &stdout_path,
        Duration::from_secs(2),
        Duration::from_secs_f64(duration + 180.0),
    );
    let stdout = fs::read_to_string(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    for line in stdout.lines().filter(|line| line.starts_with("PLAY_PATH")) {
        println!("{line}");
    }
    assert!(!stalled, "watchdog failed, see {}", stdout_path.display());
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    assert!(
        status.success(),
        "Godot exited {status}, logs in {}",
        output_directory.display()
    );
    validate(&stdout, &policy, duration)
        .unwrap_or_else(|error| panic!("{error}\nlogs in {}", output_directory.display()));
}
