// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! UI-23 boundary check for SWF action blocks retained by the art converter.

use soe_swf_convert::convert;
use swf::{Header, Sprite, Tag};

#[test]
fn ui_23_conversion_counts_root_and_nested_action_blocks_without_dropping_them() {
    let mut header = Header::default_with_swf_version(9);
    header.num_frames = 1;
    let mut bytes = Vec::new();
    swf::write_swf(
        &header,
        &[
            Tag::DoAction(&[0]),
            Tag::DefineSprite(Sprite {
                id: 7,
                num_frames: 1,
                tags: vec![Tag::DoAction(&[0]), Tag::ShowFrame, Tag::End],
            }),
            Tag::ShowFrame,
            Tag::End,
        ],
        &mut bytes,
    )
    .unwrap();
    let movie = convert(&bytes).unwrap();
    assert_eq!(movie.action_blocks, 2);
    assert_eq!(movie.timelines[&7].declared_frames, 1);
}
