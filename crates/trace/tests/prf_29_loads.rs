// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-29 load ownership, exclusive accounting and diagnostic export.

use soe_trace::{LoadCause, LoadScope, LoadToken, LoadTrace, Trace};
use std::time::{Duration, Instant};

#[test]
fn prf_29_nested_causes_keep_object_and_exclusive_time() {
    let operation = LoadScope::operation("prf_29_nested", "cell:fixture", Duration::ZERO);
    let id = operation.id();
    {
        let _object = LoadScope::object("base:Skyrim.esm:123");
        let _parse = LoadScope::new("prf_29_parse", LoadCause::Parse, "mesh.nif");
        let _io = LoadScope::new("prf_29_io", LoadCause::IoWait, "archive.bsa:mesh.nif");
    }
    drop(operation);
    let events = LoadTrace::events();
    let root = events.iter().find(|event| event.id == id).unwrap();
    let object = events
        .iter()
        .find(|event| event.parent == Some(id))
        .unwrap();
    let parse = events
        .iter()
        .find(|event| event.parent == Some(object.id))
        .unwrap();
    let io = events
        .iter()
        .find(|event| event.parent == Some(parse.id))
        .unwrap();
    assert_eq!(parse.exclusive_ns, parse.duration_ns - io.duration_ns);
    assert_eq!(root.exclusive_ns, root.duration_ns - object.duration_ns);
    assert_eq!(io.object, "base:Skyrim.esm:123");
    assert_eq!(io.root, id);
    let text = LoadTrace::operation_report(root);
    assert!(
        text.contains("over_budget=true") && text.contains("cause="),
        "{text}"
    );
}

#[test]
fn prf_29_workers_keep_parent_without_charging_worker_time_to_caller() {
    let operation = LoadScope::operation("prf_29_workers", "cell:workers", Duration::from_secs(1));
    let id = operation.id();
    let token = LoadToken::current().unwrap();
    let submitted = Instant::now();
    std::thread::spawn(move || {
        let _ownership = token.enter();
        LoadTrace::queue_wait("queued mesh", submitted);
        let _build = LoadScope::new("prf_29_worker_build", LoadCause::CpuBuild, "worker.nif");
    })
    .join()
    .unwrap();
    drop(operation);
    let events = LoadTrace::events();
    let root = events.iter().find(|event| event.id == id).unwrap();
    assert_eq!(root.exclusive_ns, root.duration_ns);
    let worker = events
        .iter()
        .find(|event| event.root == id && event.cause == Some(LoadCause::CpuBuild))
        .unwrap();
    assert_eq!(worker.parent, Some(id));
    assert_ne!(worker.thread, root.thread);
    assert!(events
        .iter()
        .any(|event| event.root == id && event.cause == Some(LoadCause::WorkerQueue)));
}

#[test]
fn prf_29_unwind_restores_ownership_and_failed_phase_is_retained() {
    let operation = LoadScope::operation("prf_29_failure", "failed cell", Duration::ZERO);
    let id = operation.id();
    let result = std::panic::catch_unwind(|| {
        let _parse = LoadScope::new("prf_29_failed_parse", LoadCause::Parse, "broken.nif");
        panic!("injected malformed load");
    });
    assert!(result.is_err());
    {
        let _after = LoadScope::new("prf_29_after_failure", LoadCause::Decode, "next.dds");
    }
    drop(operation);
    let events = LoadTrace::events();
    assert!(events
        .iter()
        .any(|event| event.phase == "prf_29_failed_parse" && event.parent == Some(id)));
    assert!(events
        .iter()
        .any(|event| event.phase == "prf_29_after_failure" && event.parent == Some(id)));
    assert!(LoadToken::current().is_none());
}

#[test]
fn prf_29_budget_log_and_dump_name_the_same_slowest_asset_and_cause() {
    let operation = LoadScope::operation("prf_29_budget", "cell:budget", Duration::ZERO);
    let id = operation.id();
    {
        let _object = LoadScope::object("base:fixture:7");
        let _parse = LoadScope::new("prf_29_slowest", LoadCause::Parse, "slow\"mesh.nif");
    }
    drop(operation);
    let events = LoadTrace::events();
    let root = events.iter().find(|event| event.id == id).unwrap();
    let report = LoadTrace::operation_report(root);
    assert!(
        report.contains("slow\\\"mesh.nif")
            && report.contains("cause=parse")
            && report.contains("base:fixture:7"),
        "{report}"
    );
    assert!(Trace::summary_text().contains(&report));
    let path = std::env::temp_dir().join(format!("soe-prf-29-{}.json", std::process::id()));
    Trace::export_chrome_json(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"cat\":\"load.parse\"") && text.contains("slow\\\"mesh.nif"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn prf_29_cause_names_cover_the_declared_contract() {
    assert_eq!(
        [
            LoadCause::IoWait,
            LoadCause::Parse,
            LoadCause::Decode,
            LoadCause::CpuBuild,
            LoadCause::GpuUpload,
            LoadCause::ShaderCompile,
            LoadCause::PipelineCompile,
            LoadCause::WorkerQueue,
        ]
        .map(LoadCause::name),
        [
            "io_wait",
            "parse",
            "decode",
            "cpu_build",
            "gpu_upload",
            "shader_compile",
            "pipeline_compile",
            "worker_queue",
        ]
    );
}

#[test]
fn prf_29_accumulated_io_subtracts_once_and_exports_as_a_counter() {
    let operation = LoadScope::operation("prf_29_accumulated", "archive index", Duration::ZERO);
    let id = operation.id();
    let started = Instant::now();
    let elapsed = started.elapsed();
    LoadTrace::accumulated("prf_29_index_io", LoadCause::IoWait, "index.bsa", elapsed);
    drop(operation);
    let events = LoadTrace::events();
    let root = events.iter().find(|event| event.id == id).unwrap();
    let io = events
        .iter()
        .find(|event| event.parent == Some(id))
        .unwrap();
    assert!(io.accumulated);
    assert_eq!(root.exclusive_ns, root.duration_ns - io.duration_ns);
    let path = std::env::temp_dir().join(format!("soe-prf-29-counter-{}.json", std::process::id()));
    Trace::export_chrome_json(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("prf_29_index_io index.bsa\",\"cat\":\"load.io_wait\",\"ph\":\"C\""));
    std::fs::remove_file(path).unwrap();
}
