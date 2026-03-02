pub const DEFAULT_NUM_BUCKETS: usize = 512;

#[derive(Debug, Clone)]
pub struct WaveformPeaks {
    pub peaks: Vec<(f32, f32)>,
    pub num_buckets: usize,
}

pub fn compute_peaks(samples: &[f32], channels: usize, num_buckets: usize) -> WaveformPeaks {
    let channels = channels.max(1);
    let num_frames = samples.len() / channels;

    if num_frames == 0 || num_buckets == 0 {
        return WaveformPeaks {
            peaks: vec![(0.0, 0.0); num_buckets],
            num_buckets,
        };
    }

    let frames_per_bucket = (num_frames as f64 / num_buckets as f64).max(1.0);
    let inv_channels = 1.0 / channels as f32;
    let mut peaks = Vec::with_capacity(num_buckets);

    for bucket in 0..num_buckets {
        let start = (bucket as f64 * frames_per_bucket) as usize;
        let end = (((bucket + 1) as f64 * frames_per_bucket) as usize).min(num_frames);

        let mut min = f32::MAX;
        let mut max = f32::MIN;

        for frame in start..end {
            let base = frame * channels;
            let mut mono = 0.0f32;
            for ch in 0..channels {
                mono += samples[base + ch];
            }
            mono *= inv_channels;
            min = min.min(mono);
            max = max.max(mono);
        }

        if min == f32::MAX {
            min = 0.0;
            max = 0.0;
        }

        peaks.push((min.clamp(-1.0, 1.0), max.clamp(-1.0, 1.0)));
    }

    WaveformPeaks { peaks, num_buckets }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_returns_zeroed_peaks() {
        let peaks = compute_peaks(&[], 1, 8);
        assert_eq!(peaks.num_buckets, 8);
        assert!(peaks.peaks.iter().all(|&(lo, hi)| lo == 0.0 && hi == 0.0));
    }

    #[test]
    fn mono_sine_has_expected_range() {
        let n = 1024;
        let samples: Vec<f32> = (0..n)
            .map(|i| (i as f32 / n as f32 * std::f32::consts::TAU).sin())
            .collect();
        let peaks = compute_peaks(&samples, 1, 16);
        assert_eq!(peaks.peaks.len(), 16);
        let global_min = peaks.peaks.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let global_max = peaks.peaks.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        assert!(global_min < -0.9);
        assert!(global_max > 0.9);
    }

    #[test]
    fn stereo_averages_channels() {
        // Left = 1.0, Right = -1.0 → mono average = 0.0
        let samples = vec![1.0f32, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
        let peaks = compute_peaks(&samples, 2, 2);
        for &(lo, hi) in &peaks.peaks {
            assert!((lo - 0.0).abs() < 1e-6);
            assert!((hi - 0.0).abs() < 1e-6);
        }
    }

    #[test]
    fn hot_samples_are_clamped() {
        let samples = vec![2.0f32, -3.0, 0.5, 0.5];
        let peaks = compute_peaks(&samples, 1, 2);
        assert_eq!(peaks.peaks[0], (-1.0, 1.0));
    }
}
