// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-20 plugin attribution uses the retained ESM record capacities.

use super::*;
use soe_esm::{FileHeader, Plugin};

#[test]
fn prf_20_record_memory_counts_losing_overrides_in_the_defining_plugin() {
    let mut original = Plugin::new(FileHeader::default());
    original.push_top(Record::new(Tag::new(b"STAT"), 1).sub(b"DATA", vec![0; 4096]));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(Record::new(Tag::new(b"STAT"), 1).sub(b"DATA", vec![0; 13]));
    let mut order = LoadOrder::new();
    order.add("Base.esm", original).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let source = DataSource {
        order: Arc::new(order),
        vfs: Arc::new(Vfs::new()),
        threads: 1,
        workers: WorkerPools::default(),
        textures: TextureCache::default(),
        language: "english".into(),
    };
    let bytes = source.record_memory_by_plugin();
    assert_eq!(bytes.len(), 2);
    assert!(bytes["Base.esm"] >= 4096);
    assert!(bytes["Patch.esp"] >= 13);
    assert!(bytes["Base.esm"] > bytes["Patch.esp"]);
    assert_eq!(
        source
            .order
            .winner(GlobalFormId { file: 0, object: 1 })
            .unwrap()
            .find(Tag::new(b"DATA"))
            .unwrap()
            .len(),
        13
    );
}

#[test]
fn prf_20_worker_pool_propagates_cell_costs_without_cross_cell_leaks() {
    let recorder = soe_trace::LoadRecorder::default();
    {
        let _capture = recorder.enter();
        WorkerPools::default()
            .ordered_map(&[0; 16], 4, |_| {
                let _measure = soe_trace::LoadMeasure::new(
                    soe_trace::LoadStage::NifParse,
                    "meshes/shared.nif",
                );
            })
            .unwrap();
    }
    assert!(recorder.report().stage_ns[soe_trace::LoadStage::NifParse as usize] > 0);
    assert!(soe_trace::LoadRecorder::current().is_none());
    let next = soe_trace::LoadRecorder::default();
    assert!(next.report().asset_ns.is_empty());
}
