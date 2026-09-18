// SPDX-License-Identifier: GPL-3.0-or-later

//! The one buffer every decoder in this crate produces, so a caller can treat
//! a vanilla xWMA line and a mod's Ogg file the same way.

/// Interleaved 32-bit float samples, the shape a host mixer wants.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved: frame 0 channel 0, frame 0 channel 1, frame 1 channel 0,
    /// and so on.
    pub samples: Vec<f32>,
}

impl Pcm {
    /// An empty buffer that still carries its stream shape.
    #[must_use]
    pub fn empty(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            samples: Vec::new(),
        }
    }

    /// Samples per channel. Zero channels reads as zero frames rather than
    /// dividing by zero.
    #[must_use]
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    /// Length in seconds, zero when the stream shape is degenerate.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.frames() as f64 / f64::from(self.sample_rate)
        }
    }

    /// True when no sample was decoded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Peak absolute sample, which the tests use to tell real audio from a
    /// buffer of digital silence.
    #[must_use]
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::Pcm;

    #[test]
    fn frames_and_duration_follow_the_channel_count() {
        let pcm = Pcm {
            sample_rate: 44100,
            channels: 2,
            samples: vec![0.0; 8],
        };
        assert_eq!(pcm.frames(), 4);
        assert!((pcm.duration_seconds() - 4.0 / 44100.0).abs() < 1e-12);
    }

    #[test]
    fn a_degenerate_shape_does_not_divide_by_zero() {
        let pcm = Pcm {
            sample_rate: 0,
            channels: 0,
            samples: vec![1.0],
        };
        assert_eq!(pcm.frames(), 0);
        assert!((pcm.duration_seconds() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn peak_reports_the_largest_magnitude() {
        let pcm = Pcm {
            sample_rate: 8000,
            channels: 1,
            samples: vec![0.1, -0.7, 0.3],
        };
        assert!((pcm.peak() - 0.7).abs() < 1e-6);
        assert!(Pcm::empty(8000, 1).is_empty());
    }
}
