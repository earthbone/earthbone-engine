// SPDX-License-Identifier: GPL-3.0-or-later

//! Paired LE/SE files stay external. Set SOE_NEMESIS_CACHE to Nemesis's
//! `test environment/cached_behaviors` directory to exercise both layouts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use soe_hkx::behavior::{BehaviorGraph, NodeKind};
use soe_hkx::character::CharacterData;
use soe_hkx::Packfile;

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().unwrap() != "SSE" {
                files(&path, out);
            }
        } else if path.extension().is_some_and(|ext| ext == "hkx") {
            out.push(path);
        }
    }
}

/// Raw addresses are storage details. Preserve the pointed-to class and
/// nullness separately so normalizing an address cannot hide a lost fixup.
fn canonical_graph(pf: &Packfile) -> (BehaviorGraph, Vec<Option<String>>) {
    let mut graph = BehaviorGraph::from_packfile(pf).unwrap();
    let mut classes = Vec::new();
    let mut normalize = |offset: &mut Option<u32>| {
        classes.push(offset.map(|at| pf.objects.get(&at).unwrap().clone()));
        *offset = offset.map(|_| 0);
    };
    if let Some(values) = &mut graph.data.variable_initial_values {
        for offset in &mut values.variant_values {
            normalize(offset);
        }
    }
    for node in &mut graph.nodes {
        node.offset = 0;
        if let NodeKind::BoneSwitch { children, .. } = &mut node.kind {
            for offset in children {
                normalize(offset);
            }
        }
    }
    (graph, classes)
}

#[test]
fn nemesis_le_and_se_have_the_same_typed_models() {
    let Some(cache) = std::env::var_os("SOE_NEMESIS_CACHE") else {
        return;
    };
    let cache = PathBuf::from(cache);
    let mut paths = Vec::new();
    files(&cache, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 37, "Nemesis cache pair count changed");
    let mut graphs = 0;
    let mut characters = 0;
    let mut nodes = 0;
    for path in paths {
        let relative = path.strip_prefix(&cache).unwrap();
        let le = Packfile::parse(&std::fs::read(&path).unwrap()).unwrap();
        let se =
            Packfile::parse(&std::fs::read(cache.join("SSE").join(relative)).unwrap()).unwrap();
        assert_eq!(le.header.pointer_size, 4);
        assert_eq!(se.header.pointer_size, 8);
        if path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .starts_with("default")
        {
            let mut left = CharacterData::from_packfile(&le).unwrap();
            let mut right = CharacterData::from_packfile(&se).unwrap();
            let mut pointed_classes = Vec::new();
            for (model, pf) in [(&mut left, &le), (&mut right, &se)] {
                let mut classes = Vec::new();
                if let Some(values) = &mut model.character_property_values {
                    for at in values.variant_values.iter_mut().flatten() {
                        classes.push(pf.objects.get(at).unwrap().clone());
                        *at = 0;
                    }
                }
                pointed_classes.push(classes);
            }
            assert_eq!(pointed_classes[0], pointed_classes[1]);
            assert_eq!(left, right, "{}", relative.display());
            characters += 1;
        } else {
            let left = canonical_graph(&le);
            let right = canonical_graph(&se);
            nodes += left.0.nodes.len();
            assert_eq!(left, right, "{}", relative.display());
            graphs += 1;
        }
    }
    assert_eq!((graphs, characters), (35, 2));
    eprintln!("Nemesis LE/SE: {graphs} behavior pairs, {characters} character pairs, {nodes} nodes per architecture");
}
