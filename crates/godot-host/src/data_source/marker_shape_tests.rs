// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn marker_node_filters_its_shape_without_losing_an_ordinary_sibling() {
    use soe_nif::blocks::{Block, Node, TriShape};
    use soe_nif::BlockRef;

    let root = Node {
        children: vec![BlockRef(1), BlockRef(2)],
        ..Node::default()
    };
    let marker = Node {
        av: soe_nif::blocks::AvObject {
            net: soe_nif::blocks::ObjectNet {
                name: "EditorMarker".to_string(),
                ..soe_nif::blocks::ObjectNet::default()
            },
            ..soe_nif::blocks::AvObject::default()
        },
        children: vec![BlockRef(3)],
        ..Node::default()
    };
    let blocks = vec![
        Block::Node(root),
        Block::Node(marker),
        Block::TriShape(TriShape::default()),
        Block::TriShape(TriShape::default()),
    ];

    let filtered = editor_marker_shape_indices(&blocks);

    assert_eq!(filtered, HashSet::from([3]));
    assert!(
        !filtered.contains(&2),
        "ordinary sibling geometry must remain"
    );
}

#[test]
fn cyclic_marker_hierarchy_terminates() {
    use soe_nif::blocks::{Block, Node, TriShape};
    use soe_nif::BlockRef;

    let marker = Node {
        av: soe_nif::blocks::AvObject {
            net: soe_nif::blocks::ObjectNet {
                name: "EditorMarker".to_string(),
                ..soe_nif::blocks::ObjectNet::default()
            },
            ..soe_nif::blocks::AvObject::default()
        },
        children: vec![BlockRef(0), BlockRef(1)],
        ..Node::default()
    };
    let filtered =
        editor_marker_shape_indices(&[Block::Node(marker), Block::TriShape(TriShape::default())]);
    assert_eq!(filtered, HashSet::from([1]));
}
