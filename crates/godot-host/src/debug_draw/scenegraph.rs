// SPDX-License-Identifier: GPL-3.0-or-later

//! NIF tree reporting through the existing nif.xml-based reader. OpenMW's
//! components/nif node model is the local precedent for controller/collision links.

use soe_nif::{blocks::Block, BlockRef, NifFile, Transform};
use std::collections::BTreeSet;

/// Authored hierarchy with accumulated transforms and resolved attachment types.
pub fn report(file: &NifFile) -> Vec<String> {
    let mut output = Vec::new();
    let mut stack: Vec<_> = file
        .roots
        .iter()
        .rev()
        .map(|root| (*root, 0, Transform::default()))
        .collect();
    let mut visited = BTreeSet::new();
    while let Some((reference, depth, parent)) = stack.pop() {
        let Some(index) = reference.get() else {
            continue;
        };
        if !visited.insert(index) {
            output.push(format!(
                "{}[{index}] repeated or cyclic link",
                "  ".repeat(depth)
            ));
            continue;
        }
        let Some(block) = file.get(reference) else {
            output.push(format!("[{index}] missing block"));
            continue;
        };
        let (av, children) = match block {
            Block::Node(node) => (&node.av, node.children.as_slice()),
            Block::TriShape(shape) => (&shape.av, &[][..]),
            Block::BsTriShape(shape) => (&shape.av, &[][..]),
            _ => {
                output.push(format!(
                    "{}[{index}] {}",
                    "  ".repeat(depth),
                    original_type(file, block, index)
                ));
                continue;
            }
        };
        let transform = parent.then(&av.transform);
        output.push(format!(
            "{}[{index}] {} {} local={:?} accumulated={transform:?}",
            "  ".repeat(depth),
            original_type(file, block, index),
            av.name(),
            av.transform
        ));
        if !av.collision.is_none() {
            output.push(format!(
                "{}collision={:?} {}",
                "  ".repeat(depth + 1),
                av.collision,
                file.get(av.collision).map_or("missing", Block::type_name)
            ));
        }
        let mut controller = av.net.controller;
        let mut seen = BTreeSet::new();
        while let Some(index) = controller.get() {
            if !seen.insert(index) {
                output.push("controller cycle".into());
                break;
            }
            let Some(block) = file.get(controller) else {
                output.push(format!("controller [{index}] missing"));
                break;
            };
            output.push(format!(
                "{}controller [{index}] {}",
                "  ".repeat(depth + 1),
                original_type(file, block, index)
            ));
            controller = match block {
                Block::TransformController(controller) => controller.time.next_controller,
                Block::MaterialColorController(controller) => controller.time.next_controller,
                _ => BlockRef::NONE,
            };
        }
        stack.extend(
            children
                .iter()
                .rev()
                .map(|child| (*child, depth + 1, transform)),
        );
    }
    output
}

fn original_type<'a>(file: &'a NifFile, block: &'a Block, index: usize) -> &'a str {
    let name = file.header.block_type(index);
    if name.is_empty() {
        block.type_name()
    } else {
        name
    }
}
