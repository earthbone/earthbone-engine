// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::sync::LazyLock;

use divan::{black_box, Bencher};
use soe_bsa::archive::{Archive, BsaVersion};
use soe_bsa::writer::{write_bsa, BsaFile};
use soe_dds::header::{caps, flags, pf, Dx10, Header, PixelFormat};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_navmesh::graph::Triangle;
use soe_navmesh::{NavGraph, NodeId};
use soe_papyrus::asm::{i, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{NativeReturn, Value, Vm};

const DDS_BC7: u32 = 98;
const FORM_BATCH: usize = 1024;
const RESOLVE_BATCH: usize = 1024;
const LOCATE_BATCH: usize = 1024;
const PATH_BATCH: usize = 128;
const NIF_BATCH: usize = 128;
const DDS_BATCH: usize = 1;
const BSA_BATCH: usize = 128;
static DDS_BYTES: LazyLock<Vec<u8>> = LazyLock::new(bc7_fixture);
static PEX_CALL: LazyLock<Vec<u8>> = LazyLock::new(call_script);
static PEX_NATIVE: LazyLock<Vec<u8>> = LazyLock::new(native_script);
static NIF_BYTES: LazyLock<Vec<u8>> = LazyLock::new(nif_fixture);
static BSA_BYTES: LazyLock<Vec<u8>> = LazyLock::new(|| {
    write_bsa(
        &[BsaFile {
            path: "meshes/bench.nif",
            bytes: &NIF_BYTES,
        }],
        BsaVersion::SkyrimSe,
    )
    .expect("synthetic BSA is valid")
});
static DATA_NIF: LazyLock<Option<Vec<u8>>> = LazyLock::new(|| {
    let (archive, name) = DATA_BSA.as_ref()?;
    archive.read(name).ok()
});
static DATA_BSA: LazyLock<Option<(Archive, String)>> = LazyLock::new(|| {
    let data = std::env::var_os("SKYRIM_DATA")?;
    let mut archives: Vec<_> = std::fs::read_dir(data)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bsa"))
                && path.file_name().is_some_and(|name| {
                    name.to_string_lossy().to_ascii_lowercase().contains("mesh")
                })
        })
        .collect();
    archives.sort();
    let path = archives.first()?;
    let archive = Archive::open(path).ok()?;
    let name = archive
        .files()
        .iter()
        .find(|name| name.to_ascii_lowercase().ends_with(".nif"))?
        .clone();
    Some((archive, name))
});

fn load_order() -> (LoadOrder, GlobalFormId) {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag::new(b"STAT"), 0x800).zsub(Tag::new(b"EDID"), "BenchRock"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag::new(b"STAT"), 0x0000_0800));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).expect("base plugin loads");
    order.add("Patch.esp", patch).expect("override loads");
    (
        order,
        GlobalFormId {
            file: 0,
            object: 0x800,
        },
    )
}

fn graph() -> NavGraph {
    let mut graph = NavGraph::new();
    let triangles = vec![
        Triangle {
            vertices: [0, 1, 2],
            neighbors: [None, None, None],
            blocked: false,
            flags: 0,
        },
        Triangle {
            vertices: [1, 3, 2],
            neighbors: [None, None, None],
            blocked: false,
            flags: 0,
        },
    ];
    graph.add_mesh(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        triangles,
    );
    graph.link(NodeId::new(0, 0), 1, NodeId::new(0, 1));
    graph
}

fn call_script() -> Vec<u8> {
    ScriptBuilder::new("Prf15Call", "")
        .function(
            FunctionBuilder::new("Run", "Int")
                .ins(soe_papyrus::Opcode::Return, vec![i(7)])
                .build(),
        )
        .bytes()
}

fn native_script() -> Vec<u8> {
    ScriptBuilder::new("Prf15Native", "")
        .function(
            FunctionBuilder::new("Run", "None")
                .ins(
                    soe_papyrus::Opcode::CallStatic,
                    vec![s("Host"), s("Ping"), soe_papyrus::asm::ident("::nonevar")],
                )
                .ins(soe_papyrus::Opcode::Return, vec![none()])
                .build(),
        )
        .bytes()
}

fn vm_native() -> Vm {
    let mut vm = Vm::new();
    vm.load_bytes(&PEX_NATIVE).expect("native fixture loads");
    vm.natives.register_global(
        "Host",
        "Ping",
        Box::new(|_| NativeReturn::Value(Value::None)),
    );
    vm
}

fn bc7_fixture() -> Vec<u8> {
    let header = Header {
        flags: flags::CAPS | flags::HEIGHT | flags::WIDTH | flags::PIXELFORMAT,
        height: 128,
        width: 128,
        pitch_or_linear_size: 0,
        depth: 0,
        mip_map_count: 0,
        pixel_format: PixelFormat {
            flags: pf::FOURCC,
            four_cc: *b"DX10",
            rgb_bit_count: 0,
            r_mask: 0,
            g_mask: 0,
            b_mask: 0,
            a_mask: 0,
        },
        caps: caps::TEXTURE,
        caps2: 0,
        dx10: Some(Dx10 {
            dxgi_format: DDS_BC7,
            dimension: 3,
            misc_flag: 0,
            array_size: 1,
            misc_flags2: 0,
        }),
    };
    let mut bytes = header.to_bytes();
    for _ in 0..(128 / 4 * 128 / 4) {
        bytes.extend_from_slice(&[
            0x40, 0x20, 0x10, 0x00, 0xff, 0xff, 0xff, 0xff, 0x55, 0x55, 0x55, 0x55, 0xaa, 0xaa,
            0xaa, 0xaa,
        ]);
    }
    bytes
}

// A valid, small NIF header plus an empty root node written by the test fixture
// format helper. This deliberately measures parsing, not file I/O.
fn nif_fixture() -> Vec<u8> {
    let mut node = Vec::new();
    node.extend_from_slice(&0u32.to_le_bytes());
    node.extend_from_slice(&0u32.to_le_bytes());
    node.extend_from_slice(&(-1i32).to_le_bytes());
    node.extend_from_slice(&0xEu32.to_le_bytes());
    for value in [
        0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0,
    ] {
        node.extend_from_slice(&value.to_le_bytes());
    }
    node.extend_from_slice(&(-1i32).to_le_bytes());
    node.extend_from_slice(&0u32.to_le_bytes());
    node.extend_from_slice(&0u32.to_le_bytes());

    let mut bytes = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
    bytes.extend_from_slice(&0x1402_0007u32.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&12u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&100u32.to_le_bytes());
    bytes.push(10);
    bytes.extend_from_slice(b"soe-bench\0");
    bytes.push(1);
    bytes.push(0);
    bytes.push(1);
    bytes.push(0);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&6u32.to_le_bytes());
    bytes.extend_from_slice(b"NiNode");
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&(node.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(b"Root");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&node);
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes
}

fn main() {
    divan::main();
}

#[divan::bench]
fn form_lookup(bencher: Bencher) {
    let (order, id) = load_order();
    bencher.bench(|| {
        for _ in 0..FORM_BATCH {
            black_box(order.winner_ref(black_box(id)));
        }
    });
}

#[divan::bench]
fn winning_override_resolve(bencher: Bencher) {
    let (order, _) = load_order();
    bencher.bench(|| {
        for _ in 0..RESOLVE_BATCH {
            let id = black_box(order.resolve(black_box(1), black_box(0x0000_0800)));
            black_box(order.winner(black_box(id))).map(|record| record.form_id);
        }
    });
}

#[divan::bench]
fn navmesh_locate(bencher: Bencher) {
    let graph = graph();
    bencher.bench(|| {
        for _ in 0..LOCATE_BATCH {
            black_box(graph.locate(black_box([0.25, 0.25, 0.0])));
        }
    });
}

#[divan::bench]
fn navmesh_find_path(bencher: Bencher) {
    let graph = graph();
    bencher.bench(|| {
        for _ in 0..PATH_BATCH {
            black_box(graph.find_path(black_box(NodeId::new(0, 0)), black_box(NodeId::new(0, 1))));
        }
    });
}

#[divan::bench]
fn papyrus_call(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let mut vm = Vm::new();
            vm.load_bytes(&PEX_CALL).expect("call fixture loads");
            vm
        })
        .bench_values(|mut vm| {
            let tid = vm
                .call_global(black_box("Prf15Call"), black_box("Run"), &[])
                .expect("call starts");
            black_box(vm.run_until_idle(64));
            black_box(vm.thread_result(tid))
        });
}

#[divan::bench]
fn papyrus_native_call(bencher: Bencher) {
    bencher.with_inputs(vm_native).bench_values(|mut vm| {
        let tid = vm
            .call_global(black_box("Prf15Native"), black_box("Run"), &[])
            .expect("call starts");
        black_box(vm.run_until_idle(64));
        black_box(vm.thread_result(tid))
    });
}

#[divan::bench]
fn nif_parse() -> soe_nif::NifFile {
    let mut parsed = None;
    for _ in 0..NIF_BATCH {
        parsed = Some(soe_nif::parse(black_box(&NIF_BYTES)).expect("synthetic NIF parses"));
    }
    parsed.expect("NIF batch runs")
}

#[divan::bench]
fn dds_bc7_decode(bencher: Bencher) {
    let dds = soe_dds::Dds::parse(&DDS_BYTES).expect("synthetic BC7 DDS parses");
    bencher.bench(|| {
        for _ in 0..DDS_BATCH {
            black_box(dds.decode(0, 0).expect("BC7 decode succeeds"));
        }
    });
}

#[divan::bench]
fn bsa_file_read(bencher: Bencher) {
    let archive = Archive::from_bytes(std::path::Path::new("bench.bsa"), BSA_BYTES.clone())
        .expect("BSA fixture opens");
    bencher.bench(|| {
        for _ in 0..BSA_BATCH {
            black_box(
                archive
                    .read(black_box("meshes/bench.nif"))
                    .expect("fixture read succeeds"),
            );
        }
    });
}

#[divan::bench]
fn installed_nif_parse(bencher: Bencher) {
    let Some(bytes) = DATA_NIF.as_ref() else {
        assert!(
            std::env::var_os("SKYRIM_DATA").is_none(),
            "SKYRIM_DATA is set but no mesh NIF was found"
        );
        return;
    };
    bencher.bench(|| soe_nif::parse(black_box(bytes)).expect("installed NIF parses"));
}

#[divan::bench]
fn installed_bsa_file_read(bencher: Bencher) {
    let Some((archive, name)) = DATA_BSA.as_ref() else {
        assert!(
            std::env::var_os("SKYRIM_DATA").is_none(),
            "SKYRIM_DATA is set but no mesh archive was found"
        );
        return;
    };
    bencher.bench(|| {
        archive
            .read(black_box(name))
            .expect("installed BSA file reads")
    });
}
