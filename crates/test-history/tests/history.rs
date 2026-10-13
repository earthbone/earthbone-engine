//! Synthetic evidence contracts derived from the run producers, without user data.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::json;
use soe_test_history::{parse, Result, Row};

fn ids() -> BTreeSet<String> {
    ["PLY-03", "TST-19"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn cargo_results_preserve_identity_failure_location_and_backlog() -> Result<()> {
    let text = "Running unittests src/lib.rs (target/debug/deps/soe_game-123456)\ntest routes::ply_03_walk ... FAILED\ntest tst_19_check ... ignored\n---- routes::ply_03_walk stdout ----\nthread 'routes::ply_03_walk' panicked at crates/game/src/route.rs:42:3:\nexpected 192 but found 12 at 0xfeed\ntest result: FAILED. 0 passed; 1 failed; 1 ignored; finished in 2.5s\n";
    let result = parse::text_log(Path::new("tests.log"), text, &ids())?;
    assert_eq!(result.rows["tests"][0]["backlog_row"], "PLY-03");
    assert_eq!(result.rows["tests"][0]["package"], "soe-game");
    assert_eq!(
        result.rows["failures"][0]["file"],
        "crates/game/src/route.rs"
    );
    assert_eq!(result.rows["failures"][0]["line"], 42);
    assert_eq!(result.rows["resources"][0]["seconds"], 2.5);
    Ok(())
}

#[test]
fn diagnostics_cover_text_and_cargo_json_and_warning_counts() -> Result<()> {
    let text = "error[E0123]: invalid unit\n --> crates/game/src/lib.rs:8:2\nwarning: unused variable\n{\"reason\":\"compiler-message\",\"target\":{\"name\":\"soe-game\"},\"message\":{\"level\":\"warning\",\"message\":\"unused\",\"code\":{\"code\":\"unused\"},\"spans\":[{\"is_primary\":true,\"file_name\":\"foo.rs\",\"line_start\":9}]}}\n";
    let result = parse::text_log(Path::new("clippy-all.log"), text, &ids())?;
    assert_eq!(result.rows["compile"].len(), 3);
    assert_eq!(result.rows["compile"][0]["unit"], "soe-game");
    assert_eq!(result.rows["compile"][2]["line"], 9);
    Ok(())
}

#[test]
fn play_path_retains_owner_status_capture_and_nested_numbers() -> Result<()> {
    let text = "PLAY_PATH {\"check\":\"capture\",\"owner\":\"TST-19\",\"pass\":false,\"status\":\"XFAIL\",\"stop\":\"market\",\"elapsed_s\":12.0,\"value\":{\"path\":\"market.png\",\"frame_ms\":51.2,\"path_points\":1}}\nframes=40 topics=0\n";
    let result = parse::text_log(Path::new("test.log"), text, &ids())?;
    assert_eq!(result.rows["play_checks"][0]["status"], "XFAIL");
    assert_eq!(result.rows["captures"][0]["path"], "market.png");
    assert!(result.rows["metrics"].iter().any(|r| r["key"] == "topics"));
    Ok(())
}

#[test]
fn resources_decode_rss_timing_and_earlyoom() -> Result<()> {
    let text = "1234\t8192\t20\t--script synthetic.gd\nOct 13 12:01:00 host earlyoom: killed process 1234 'godot' RSS=8192\n";
    let result = parse::text_log(Path::new("godot-rss.tsv"), text, &ids())?;
    assert_eq!(result.rows["resources"][0]["peak_rss_kib"], 8192 * 1024);
    assert_eq!(result.rows["resources"][1]["earlyoom"], true);
    Ok(())
}

#[test]
fn agent_events_replies_and_status_keep_cost_and_merge_evidence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let events = directory.path().join("fixer.events.jsonl");
    std::fs::write(&events, "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":120,\"output_tokens\":8}}\n{\"type\":\"error\",\"message\":\"at capacity\"}\n")?;
    std::fs::write(
        directory.path().join("fixer.last"),
        "{\"status\":\"partial\",\"commit\":\"abc\",\"fixed\":[\"a\"],\"left\":[\"b\"]}",
    )?;
    let result = parse::file(&events, &ids(), &Row::new())?;
    assert_eq!(result.rows["agents"][0]["tokens"]["input_tokens"], 120);
    assert_eq!(result.rows["agents"][0]["reply_status"], "partial");
    let status = "12:00:00 integration begin lead/test at abc123, 3 groups merged, 0 skipped\n12:00:05 integration fixer fix-a model exit 0 in 2 min\n12:02:00 integration loop merged fix-a at def456\n12:03:00 integration loop fixer fix-b conflicts with the integration tip, left out\n";
    let result = parse::text_log(Path::new("status.log"), status, &ids())?;
    assert_eq!(result.rows["agents"][0]["minutes"], 2);
    assert!(result.rows["merges"].iter().any(|r| r["conflict"] == true));
    Ok(())
}

#[test]
fn unknown_rows_and_missing_globs_are_rejected_and_fingerprints_are_stable() -> Result<()> {
    let result = parse::text_log(
        Path::new("tests.log"),
        "test ply_99_missing ... ok\n",
        &ids(),
    )?;
    assert_eq!(result.rows["tests"][0]["backlog_row"], json!(null));
    assert_eq!(
        parse::fingerprint("address 0xface expected 192 got 12")?,
        parse::fingerprint("address 0xabcd expected 200 got 13")?
    );
    assert_eq!(
        parse::fingerprint("12.3ms at 0XABCD, 1.4e-8 seconds")?,
        parse::fingerprint("15.4ms at 0XFACE, 5.1e-9 seconds")?
    );
    assert!(parse::discover(&["/nonexistent/synthetic-*-history.log".to_owned()]).is_err());
    Ok(())
}

#[test]
fn printed_skip_signal_timeout_and_retry_reasons_are_preserved() -> Result<()> {
    let text = "Running unittests src/lib.rs (target/debug/deps/soe_game-1234)\ntest ply_03_walk ...\nSKIP missing synthetic data\nok\nretrying on one thread --test-threads=1\nprocess didn't exit successfully: `/deps/it-1234` (signal: 9, SIGKILL)\ntimed out after 120 s\nSKYRIM_DATA=\"/synthetic/Game Data\" peak_ms=12\n";
    let result = parse::text_log(Path::new("tests.log"), text, &ids())?;
    let tests = &result.rows["tests"];
    assert_eq!(tests[0]["status"], "skip");
    assert_eq!(tests[1]["status"], "retry-one-thread");
    assert_eq!(tests[2]["status"], "killed");
    assert_eq!(tests[3]["status"], "timeout");
    assert_eq!(result.run["skyrim_data"], "/synthetic/Game Data");
    Ok(())
}

#[test]
fn status_recovers_historical_steps_without_attributing_stale_logs() -> Result<()> {
    let text = "12:00:00 integration begin lead/test at abc123, 1 groups merged, 0 skipped\n12:01:00 integration tests exit 101, 0 units, 1 failed tests\n12:02:00 integration begin lead/test at def456, 1 groups merged, 0 skipped\n12:03:00 integration build exit 0, 0 units, 0 failed tests\n";
    let steps = parse::status_runs(Path::new("status.log"), text)?;
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].run["commit_id"], "abc123");
    assert!(parse::current_stage_facts(text, "tests.log")?.is_empty());
    assert_eq!(
        parse::current_stage_facts(text, "build.log")?["commit_id"],
        "def456"
    );
    Ok(())
}

#[test]
fn codex_command_output_is_not_counted_again_as_cargo_failures() -> Result<()> {
    let event = json!({"type":"item.completed","item":{"type":"command_execution","output":"thread panicked at fixture.rs:12:3:\nFAILED test fixture\n","exit_code":101}});
    let result = parse::text_log(Path::new("fix.events.jsonl"), &format!("{event}\n"), &ids())?;
    assert!(!result.rows.contains_key("failures"));
    assert_eq!(result.rows["agents"].len(), 1);
    Ok(())
}

#[test]
fn long_failure_blocks_are_retained_with_fixed_size_fingerprints() -> Result<()> {
    let body = "fixture detail for an assertion\n".repeat(4096);
    let text = format!("test ply_03_walk ... FAILED\n---- ply_03_walk stdout ----\nthread panicked at fixture.rs:1:2:\n{body}");
    let result = parse::text_log(Path::new("tests.log"), &text, &ids())?;
    let failure = &result.rows["failures"][0];
    assert_eq!(failure["fingerprint"].as_str().map(str::len), Some(64));
    assert!(failure["message"]
        .as_str()
        .is_some_and(|message| message.ends_with(body.trim_end())));
    Ok(())
}

#[test]
fn inline_image_bytes_are_omitted_and_capture_paths_remain() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("fixture.events.jsonl");
    let event = json!({"type":"item.completed","result":{"content":[{"type":"image","mimeType":"image/png","data":"synthetic-bytes","path":"/synthetic/capture.png"}]}});
    std::fs::write(&path, event.to_string())?;
    let result = parse::file(&path, &ids(), &Row::new())?;
    let text = result.rows["evidence_lines"][0]["text"]
        .as_str()
        .ok_or_else(|| soe_test_history::Error::Input("missing evidence text".to_owned()))?;
    assert!(!text.contains("synthetic-bytes"));
    assert!(text.contains("/synthetic/capture.png"));
    Ok(())
}

#[cfg(feature = "store")]
mod database {
    use super::*;
    use soe_test_history::store::{HistoryStore, Mode, TursoStore};
    use soe_test_history::{row, Evidence};

    fn runtime() -> Result<tokio::runtime::Runtime> {
        Ok(tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?)
    }

    fn step(id: &str, commit: &str, parent: &str, status: &str) -> Evidence {
        let mut evidence = Evidence::new(id, "tier1");
        evidence.run.insert("commit_id".to_owned(), json!(commit));
        evidence.run.insert("parents".to_owned(), json!([parent]));
        evidence.run.insert(
            "started".to_owned(),
            json!(if commit == "parent" { "1" } else { "2" }),
        );
        evidence.push(
            "tests",
            row(&[
                ("package", json!("soe-game")),
                ("binary", json!("it")),
                ("name", json!("ply_03_walk")),
                ("status", json!(status)),
            ]),
        );
        evidence
    }

    #[test]
    fn planted_regression_flake_and_new_failure_are_found() -> Result<()> {
        runtime()?.block_on(async {
            let directory = tempfile::tempdir()?;
            let store =
                TursoStore::open(&directory.path().join("history.db"), Mode::Locked, true).await?;
            store.record(&step("old", "parent", "base", "pass")).await?;
            store
                .record(&step("new", "child", "parent", "fail"))
                .await?;
            store
                .record(&step("retry", "child", "parent", "pass"))
                .await?;
            assert_eq!(
                store
                    .query("regressions", &["child".to_owned()])
                    .await?
                    .len(),
                1
            );
            assert_eq!(store.query("flaky", &[]).await?.len(), 1);
            assert_eq!(
                store
                    .query("new-failures", &["parent".to_owned()])
                    .await?
                    .len(),
                1
            );
            Ok(())
        })
    }

    #[test]
    fn repeated_ingest_and_rebuild_are_idempotent_and_invalid_rows_rollback() -> Result<()> {
        runtime()?.block_on(async {
            let directory = tempfile::tempdir()?;
            let store =
                TursoStore::open(&directory.path().join("history.db"), Mode::Locked, true).await?;
            let evidence = step("same", "child", "parent", "pass");
            store.record(&evidence).await?;
            store.record(&evidence).await?;
            let before = store.sql("SELECT * FROM tests", &[]).await?;
            let mut invalid = evidence.clone();
            invalid.push("unknown", Row::new());
            assert!(store.record(&invalid).await.is_err());
            assert_eq!(store.sql("SELECT * FROM tests", &[]).await?, before);
            store.clear().await?;
            store.record(&evidence).await?;
            assert_eq!(store.sql("SELECT * FROM tests", &[]).await?, before);
            assert_eq!(
                store.sql("PRAGMA integrity_check", &[]).await?[0]["integrity_check"],
                "ok"
            );
            Ok(())
        })
    }

    #[test]
    fn mvcc_connections_use_concurrent_transactions() -> Result<()> {
        runtime()?.block_on(async {
            let directory = tempfile::tempdir()?;
            let store =
                TursoStore::open(&directory.path().join("history.db"), Mode::Mvcc, true).await?;
            let sibling = store.connect()?;
            store.record(&step("first", "a", "base", "pass")).await?;
            sibling.record(&step("second", "b", "a", "pass")).await?;
            assert_eq!(
                store.sql("SELECT COUNT(*) AS n FROM runs", &[]).await?[0]["n"],
                2
            );
            Ok(())
        })
    }

    #[test]
    fn installed_log_summary_matches_integration_summary() -> Result<()> {
        let Ok(logs) = std::env::var("SOE_TEST_HISTORY_LOGS") else {
            return Ok(());
        };
        runtime()?.block_on(async {
            let directory = tempfile::tempdir()?;
            let store =
                TursoStore::open(&directory.path().join("history.db"), Mode::Locked, true).await?;
            let root = Path::new(&logs).parent().ok_or_else(|| {
                soe_test_history::Error::Input("logs require a parent".to_owned())
            })?;
            let status_text = std::fs::read_to_string(root.join("status.log"))?;
            let facts = parse::integration_facts(&status_text)?;
            let summary_path = Path::new(&logs).join("integration/summary.txt");
            let summary_path = std::fs::canonicalize(summary_path)?;
            let summary_text = std::fs::read_to_string(&summary_path)?;
            let expected = parse::summary_lines(&summary_text);
            let mut summary = parse::text_log(&summary_path, &summary_text, &ids())?;
            summary.run.extend(facts.clone());
            store.record(&summary).await?;
            let files = parse::discover(std::slice::from_ref(&logs))?;
            for (index, path) in files.iter().enumerate() {
                if path == &summary_path {
                    continue;
                }
                let context = if path.parent() == summary_path.parent() {
                    parse::current_stage_facts(
                        &status_text,
                        path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
                    )?
                } else {
                    Row::new()
                };
                store.record(&parse::file(path, &ids(), &context)?).await?;
                if (index + 1).is_multiple_of(100) {
                    eprintln!("history corpus: {} of {} sources", index + 1, files.len());
                }
            }
            let commit = facts["commit_id"].as_str().ok_or_else(|| {
                soe_test_history::Error::Input("integration tip missing".to_owned())
            })?;
            let rows = store.query("summary", &[commit.to_owned()]).await?;
            let actual = rows
                .iter()
                .filter_map(|r| r["line"].as_str())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
            Ok(())
        })
    }

    #[test]
    fn cli_rebuild_retains_direct_records_and_is_idempotent() -> Result<()> {
        let root = tempfile::tempdir()?;
        let logs = root.path().join("log/integration");
        std::fs::create_dir_all(&logs)?;
        std::fs::write(root.path().join("BACKLOG.csv"), "\"id\"\n\"PLY-03\"\n")?;
        std::fs::write(logs.join("tests.log"), "test ply_03_walk ... FAILED\n")?;
        let input = root.path().join("input.json");
        std::fs::write(
            &input,
            serde_json::to_vec(&step("direct", "child", "parent", "pass"))?,
        )?;
        let database = root.path().join("history.db");
        let invoke = |args: &[&str]| -> Result<String> {
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-test-history"))
                .current_dir(root.path())
                .args(args)
                .arg("--db")
                .arg(&database)
                .args(["--mode", "multiprocess"])
                .output()?;
            if !output.status.success() {
                return Err(soe_test_history::Error::Input(
                    String::from_utf8_lossy(&output.stderr).into_owned(),
                ));
            }
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        };
        let input_path = input.to_str().ok_or_else(|| {
            soe_test_history::Error::Input("fixture path is not UTF-8".to_owned())
        })?;
        let root_path = root.path().to_str().ok_or_else(|| {
            soe_test_history::Error::Input("fixture root is not UTF-8".to_owned())
        })?;
        invoke(&["record", input_path])?;
        let log_path = logs.join("tests.log");
        let log_path = log_path
            .to_str()
            .ok_or_else(|| soe_test_history::Error::Input("fixture log is not UTF-8".to_owned()))?;
        invoke(&["record", "--log", log_path, "--commit", "abc"])?;
        invoke(&["record", "--log", log_path, "--commit", "abc"])?;
        invoke(&["rebuild", "--root", root_path])?;
        let first = invoke(&["sql", "SELECT id,commit_id FROM runs ORDER BY id"])?;
        invoke(&["rebuild", "--root", root_path])?;
        let second = invoke(&["sql", "SELECT id,commit_id FROM runs ORDER BY id"])?;
        assert_eq!(first, second);
        let counts: serde_json::Value = serde_json::from_str(&invoke(&[
            "sql",
            "SELECT COUNT(*) AS n FROM runs WHERE id='direct'",
        ])?)?;
        assert_eq!(counts[0]["n"], 1);
        let counts: serde_json::Value = serde_json::from_str(&invoke(&[
            "sql",
            "SELECT COUNT(*) AS n FROM runs WHERE commit_id='abc'",
        ])?)?;
        assert_eq!(counts[0]["n"], 2);
        Ok(())
    }

    #[test]
    fn normalized_failures_keep_first_and_last_observed_commits() -> Result<()> {
        runtime()?.block_on(async {
            let directory = tempfile::tempdir()?;
            let store =
                TursoStore::open(&directory.path().join("history.db"), Mode::Locked, true).await?;
            for (id, commit, parent, message) in [
                ("old", "parent", "base", "expected 12 at 0xface"),
                ("new", "child", "parent", "expected 13 at 0xfeed"),
            ] {
                let mut evidence = step(id, commit, parent, "fail");
                evidence.push(
                    "failures",
                    row(&[
                        ("message", json!(message)),
                        ("fingerprint", json!(parse::fingerprint(message)?)),
                    ]),
                );
                store.record(&evidence).await?;
            }
            let rows = store
                .sql(
                    "SELECT DISTINCT first_commit,last_commit FROM failures",
                    &[],
                )
                .await?;
            assert_eq!(
                rows,
                vec![row(&[
                    ("first_commit", json!("parent")),
                    ("last_commit", json!("child"))
                ])]
            );
            Ok(())
        })
    }

    #[test]
    fn cli_queries_open_while_another_process_writes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let database = root.path().join("history.db");
        runtime()?.block_on(async {
            TursoStore::open(&database, Mode::Multiprocess, true).await?;
            Ok::<_, soe_test_history::Error>(())
        })?;
        let binary = env!("CARGO_BIN_EXE_soe-test-history");
        let mut writer = std::process::Command::new(binary)
            .args(["stress-worker", "writer", "fresh-open", "2"])
            .arg(root.path())
            .arg("--db")
            .arg(&database)
            .args(["--mode", "multiprocess"])
            .spawn()?;
        std::fs::write(root.path().join("start"), "start")?;
        let mut all_succeeded = true;
        for _ in 0..10 {
            let output = std::process::Command::new(binary)
                .args(["sql", "SELECT COUNT(*) AS n FROM runs", "--db"])
                .arg(&database)
                .args(["--mode", "multiprocess"])
                .output()?;
            all_succeeded &= output.status.success();
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let status = writer.wait()?;
        assert!(all_succeeded);
        assert!(status.success());
        Ok(())
    }
}
