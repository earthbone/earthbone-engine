// SPDX-License-Identifier: GPL-3.0-or-later

//! Low-rate 44.1 kHz WMA2 frames, following FFmpeg's wma_decode_block and
//! ff_wma_init. oxideav-wma 0.0.4's noise policy covers only 22.05 kHz and
//! cannot represent a cutoff inside an exponent band. Its VLC tables and
//! synthesis remain shared. Noise-substituted bins use its zero-fill policy.

use oxideav_wma::band_partition::exponent_band_edges;
use oxideav_wma::bitio::BitReader;
use oxideav_wma::stream_config::StreamConfig;
use oxideav_wma::vendor_frame::{
    escape_level_width, ChannelBlock, Envelope, FrameParseError, ParsedBlock, ParsedFrame,
};
use oxideav_wma::wire_vlc::{coef_vlc, gain_vlc, runlevel_map, scale_vlc};

pub(super) struct Parser {
    config: StreamConfig,
    starts: Vec<u64>,
    boundary: usize,
    latch: bool,
    previous: u8,
    next: u8,
    exponents: Vec<Vec<i32>>,
}

impl Parser {
    pub(super) fn new(config: &StreamConfig, starts: &[u64]) -> Self {
        Self {
            config: config.clone(),
            starts: starts.to_vec(),
            boundary: 0,
            latch: true,
            previous: 0,
            next: 0,
            exponents: vec![Vec::new(); usize::from(config.channels)],
        }
    }

    pub(super) fn raise_latch(&mut self) {
        self.latch = true;
    }

    pub(super) fn parse_frame(
        &mut self,
        reader: &mut BitReader<'_>,
    ) -> Result<ParsedFrame, FrameParseError> {
        while self
            .starts
            .get(self.boundary)
            .is_some_and(|start| *start <= reader.position() as u64)
        {
            self.raise_latch();
            self.boundary += 1;
        }
        let mut remaining = self.config.frame_length;
        let mut blocks = Vec::new();
        while remaining > 0 {
            let block = self.block(reader, remaining)?;
            remaining -= block.block_size;
            blocks.push(block);
        }
        Ok(ParsedFrame { blocks })
    }

    fn size(&self, index: u8) -> Result<u16, FrameParseError> {
        self.config
            .block_size_for_index(index)
            .ok_or(FrameParseError::BadBlockSizeIndex { index })
    }

    fn block(
        &mut self,
        reader: &mut BitReader<'_>,
        remaining: u16,
    ) -> Result<ParsedBlock, FrameParseError> {
        let (previous, current) = if self.latch {
            self.latch = false;
            (
                reader.read_bits(self.config.w_bs)? as u8,
                reader.read_bits(self.config.w_bs)? as u8,
            )
        } else {
            (self.previous, self.next)
        };
        self.next = reader.read_bits(self.config.w_bs)? as u8;
        self.previous = current;
        let prev_size = self.size(previous)?;
        let block_size = self.size(current)?;
        let next_size = self.size(self.next)?;
        if block_size > remaining {
            return Err(FrameParseError::BlockOverflow {
                block_size,
                remaining,
            });
        }
        let joint_stereo = self.config.channels == 2 && reader.read_bit()?;
        let mut channels = (0..self.config.channels)
            .map(|_| {
                Ok(ChannelBlock {
                    coded: reader.read_bit()?,
                    envelope: None,
                    noise_flags: Vec::new(),
                    noise_gains: Vec::new(),
                    coefficients: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, FrameParseError>>()?;
        let edges = exponent_band_edges(self.config.sample_rate, block_size);
        let end = (self.config.coef_end(self.config.frame_length) >> current) as usize;
        let mut total_gain = 0;
        if channels.iter().any(|channel| channel.coded) {
            total_gain = 1;
            loop {
                let gain = reader.read_bits(7)? as u32;
                total_gain += gain;
                if gain != 127 {
                    break;
                }
            }
            let bands = noise_bands(&edges, block_size, end);
            let mut flags = vec![Vec::new(); channels.len()];
            for (channel, flags) in channels.iter().zip(&mut flags) {
                if channel.coded {
                    for _ in &bands {
                        flags.push(reader.read_bit()?);
                    }
                }
            }
            for flags in &flags {
                let count = flags.iter().filter(|flag| **flag).count();
                if count > 0 {
                    let mut gain = reader.read_bits(7)? as i32 - 19;
                    for _ in 1..count {
                        gain += i32::from(gain_vlc().decode_symbol(reader)?) - 18;
                    }
                    let _ = gain;
                }
            }
            let fresh = block_size == self.config.frame_length || reader.read_bit()?;
            for (index, channel) in channels
                .iter_mut()
                .enumerate()
                .filter(|(_, channel)| channel.coded)
            {
                if fresh {
                    let mut exponent = 36;
                    let mut expanded = vec![0; usize::from(block_size)];
                    for band in edges.windows(2) {
                        exponent += i32::from(scale_vlc().decode_symbol(reader)?) - 60;
                        expanded[usize::from(band[0])..usize::from(band[1])].fill(exponent);
                    }
                    self.exponents[index] = expanded;
                }
                let cached = &self.exponents[index];
                if cached.is_empty() {
                    return Err(FrameParseError::Vlc);
                }
                channel.envelope = Some(Envelope::Exponents(
                    edges[..edges.len() - 1]
                        .iter()
                        .map(|edge| {
                            cached[usize::from(*edge) * cached.len() / usize::from(block_size)]
                        })
                        .collect(),
                ));
            }
            for (index, channel) in channels
                .iter_mut()
                .enumerate()
                .filter(|(_, channel)| channel.coded)
            {
                let alt = joint_stereo && index == 1;
                let excluded: Vec<_> = bands
                    .iter()
                    .zip(&flags[index])
                    .filter_map(|(band, flag)| flag.then_some(*band))
                    .collect();
                let count = end
                    - excluded
                        .iter()
                        .map(|(start, end)| end - start)
                        .sum::<usize>();
                let packed = coefficients(reader, &self.config, alt, total_gain, count)?;
                channel.coefficients = vec![0; end];
                let mut values = packed.into_iter();
                for (bin, value) in channel.coefficients.iter_mut().enumerate() {
                    if !excluded
                        .iter()
                        .any(|(start, end)| (*start..*end).contains(&bin))
                    {
                        *value = values.next().ok_or(FrameParseError::Vlc)?;
                    }
                }
            }
        }
        Ok(ParsedBlock {
            block_size,
            size_index: current,
            prev_size: Some(prev_size),
            next_size: Some(next_size),
            joint_stereo,
            total_gain,
            n_coef: end as u16,
            channels,
        })
    }
}

fn noise_bands(edges: &[u16], block_size: u16, end: usize) -> Vec<(usize, usize)> {
    let cutoff = (f64::from(block_size) * 0.4 + 0.5) as usize;
    edges
        .windows(2)
        .filter_map(|band| {
            let start = usize::from(band[0]).max(cutoff);
            let end = usize::from(band[1]).min(end);
            (start < end).then_some((start, end))
        })
        .collect()
}

fn coefficients(
    reader: &mut BitReader<'_>,
    config: &StreamConfig,
    alt: bool,
    gain: u32,
    count: usize,
) -> Result<Vec<i32>, FrameParseError> {
    let vlc = coef_vlc(config.vlc_class, alt).ok_or(FrameParseError::Vlc)?;
    let map = runlevel_map(config.vlc_class, alt).ok_or(FrameParseError::Vlc)?;
    let mut output = vec![0; count];
    let mut cursor = 0;
    while cursor < count {
        let symbol = vlc.decode_symbol(reader)?;
        let (run, level) = match symbol {
            0 => {
                let level = reader.read_bits(escape_level_width(gain))? as i32;
                (reader.read_bits(config.frame_length_bits)? as usize, level)
            }
            1 => break,
            symbol => {
                let (run, level) = *map
                    .get(usize::from(symbol) - 2)
                    .ok_or(FrameParseError::SymbolOutOfRange { symbol })?;
                (usize::from(run), i32::from(level))
            }
        };
        cursor += run;
        let value = output
            .get_mut(cursor)
            .ok_or(FrameParseError::CoefficientOverrun {
                index: cursor as u32,
                n_coef: count as u16,
            })?;
        *value = if reader.read_bit()? { level } else { -level };
        cursor += 1;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_wma::bitio::BitWriter;
    use oxideav_wma::header::Version;

    #[test]
    fn aud_02_noise_cutoff_splits_the_first_high_band_and_clips_the_last() {
        assert_eq!(
            noise_bands(&[0, 300, 500, 800, 1024], 1024, 932),
            [(410, 500), (500, 800), (800, 932)]
        );
    }

    #[test]
    fn aud_02_low_rate_frame_consumes_noise_flags_before_exponents() {
        let config = StreamConfig::derive(Version::V2, 44100, 2, 4000, 1485, 31).unwrap();
        let mut writer = BitWriter::new();
        writer.write_bits(0, config.w_bs * 3);
        writer.write_bit(false);
        writer.write_bit(true);
        writer.write_bit(false);
        writer.write_bits(63, 7);
        let end = 1864;
        for _ in 0..4 {
            writer.write_bit(true);
        }
        writer.write_bits(19, 7);
        for _ in 1..4 {
            assert!(gain_vlc().encode_symbol(18, &mut writer));
        }
        for _ in 0..25 {
            assert!(scale_vlc().encode_symbol(60, &mut writer));
        }
        assert!(coef_vlc(config.vlc_class, false)
            .unwrap()
            .encode_symbol(0, &mut writer));
        writer.write_bits(7, 9);
        writer.write_bits(818, 11);
        writer.write_bit(true);
        let bits = writer.bit_len();
        let bytes = writer.into_bytes();
        let mut reader = BitReader::with_bit_len(&bytes, bits);
        let frame = Parser::new(&config, &[0]).parse_frame(&mut reader).unwrap();
        assert_eq!(reader.position(), bits);
        let mut expected = vec![0; end];
        expected[818] = 7;
        assert_eq!(frame.blocks[0].channels[0].coefficients, expected);
        assert!(!frame.blocks[0].channels[1].coded);
        for prefix in 0..bits {
            let mut reader = BitReader::with_bit_len(&bytes, prefix);
            assert!(Parser::new(&config, &[0]).parse_frame(&mut reader).is_err());
        }
    }
}
