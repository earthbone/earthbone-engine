// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_audio::wma::{Native, WmaDecoder};
use soe_audio::xwma::{WaveFormat, Xwma, WMAUDIO2, WMAUDIO3};

fn xwma(
    format_tag: u16,
    channels: u16,
    bits_per_sample: u16,
    block_align: u16,
    packet_totals: Vec<u32>,
    data: &[u8],
) -> Vec<u8> {
    Xwma {
        format: WaveFormat {
            format_tag,
            channels,
            sample_rate: 44_100,
            avg_bytes_per_sec: 4_000,
            block_align,
            bits_per_sample,
            extra: Vec::new(),
        },
        packet_totals,
        data,
    }
    .to_bytes()
}

#[test]
fn aud_02_zero_decoded_frame_size_returns_an_error_without_panicking() {
    let file = xwma(WMAUDIO2, 1, 7, 1, vec![2], &[0]);
    let result = std::panic::catch_unwind(|| Native.decode_xwma(&file));

    assert!(
        result.is_ok(),
        "malformed frame size panicked during validation"
    );
    assert!(
        result.unwrap().is_err(),
        "malformed frame size was accepted"
    );
}

#[test]
fn aud_02_rejects_packet_tables_that_do_not_match_packet_count_or_order() {
    let wrong_count = xwma(WMAUDIO2, 1, 16, 1, vec![2, 4], &[0]);
    assert!(Native.decode_xwma(&wrong_count).is_err());

    let decreasing = xwma(WMAUDIO2, 1, 16, 1, vec![4, 2], &[0, 0]);
    assert!(Native.decode_xwma(&decreasing).is_err());
}

#[test]
fn aud_02_rejects_decoded_totals_that_end_mid_frame() {
    let file = xwma(WMAUDIO2, 1, 16, 1, vec![3], &[0]);
    assert!(Native.decode_xwma(&file).is_err());
}

#[test]
fn aud_02_rejects_wma_pro_before_starting_the_wma2_decoder() {
    let file = xwma(WMAUDIO3, 1, 16, 1, vec![], &[0]);
    assert!(Native.decode_xwma(&file).is_err());
}
