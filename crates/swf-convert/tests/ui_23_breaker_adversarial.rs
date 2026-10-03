// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_swf_convert::convert;

fn tag(code: u16, data: &[u8], output: &mut Vec<u8>) {
    let header = (code << 6) | data.len() as u16;
    output.extend_from_slice(&header.to_le_bytes());
    output.extend_from_slice(data);
}

fn movie(frame_count: u16, tags: &[u8]) -> Vec<u8> {
    let mut body = vec![0x08, 0, 0, 12];
    body.extend_from_slice(&frame_count.to_le_bytes());
    body.extend_from_slice(tags);
    body.extend_from_slice(&[0, 0]);
    let mut bytes = b"FWS".to_vec();
    bytes.push(9);
    bytes.extend_from_slice(&((body.len() + 8) as u32).to_le_bytes());
    bytes.extend_from_slice(&body);
    bytes
}

#[test]
fn declared_frame_count_must_match_showframe_tags() {
    let mut tags = Vec::new();
    tag(1, &[], &mut tags);
    assert!(convert(&movie(2, &tags)).is_err());
}

#[test]
fn placing_a_character_survives_to_the_next_frame_until_removeobject2() {
    let mut tags = Vec::new();
    let mut place = vec![7, 0, 3, 0, 0];
    place.extend_from_slice(&[0]);
    tag(4, &place, &mut tags);
    tag(1, &[], &mut tags);
    tag(28, &[3, 0], &mut tags);
    tag(1, &[], &mut tags);

    let result = convert(&movie(2, &tags)).unwrap();
    let frames = &result.timelines[&0].frames;
    assert_eq!(frames[0].display_list.len(), 1);
    assert_eq!(frames[0].display_list[0].character, 7);
    assert_eq!(frames[1].display_list.as_slice(), &[]);
}

#[test]
fn modifying_an_unoccupied_depth_is_rejected_without_partial_movie_output() {
    let mut tags = Vec::new();
    tag(26, &[1, 0, 5, 0], &mut tags);
    tag(1, &[], &mut tags);
    assert!(convert(&movie(1, &tags)).is_err());
}

#[test]
fn input_and_declared_uncompressed_lengths_over_the_limit_are_rejected() {
    let mut bytes = b"FWS\x09".to_vec();
    bytes.extend_from_slice(&((128 * 1024 * 1024u32) + 1).to_le_bytes());
    assert!(convert(&bytes).is_err());
}
