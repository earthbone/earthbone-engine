#![allow(clippy::expect_used)]

use iai_callgrind::{
    library_benchmark, library_benchmark_group, main, Callgrind, EventKind, LibraryBenchmarkConfig,
};
use soe_esm::{FileHeader, Plugin, Record};
use std::hint::black_box;

const RECORDS: u32 = 128;
const MAX_INSTRUCTIONS: u64 = 500_000;

fn synthetic_plugin() -> Vec<u8> {
    let mut plugin = Plugin::new(FileHeader::default());
    for id in 1..=RECORDS {
        plugin.push_top(Record::new(*b"MISC", id).zsub(*b"EDID", &format!("Tst04_{id}")));
    }
    plugin.to_bytes()
}

#[library_benchmark(
    config = LibraryBenchmarkConfig::default().tool(
        Callgrind::default().hard_limits([(EventKind::Ir, MAX_INSTRUCTIONS)])
    )
)]
#[bench::parse_128_records(args = (), setup = synthetic_plugin)]
fn bench_plugin_parse(bytes: Vec<u8>) -> usize {
    black_box(
        Plugin::parse(&bytes)
            .expect("synthetic plugin parses")
            .records
            .len(),
    )
}

library_benchmark_group!(
    name = tst_04_esm_parser;
    benchmarks = bench_plugin_parse
);

main!(library_benchmark_groups = tst_04_esm_parser);
