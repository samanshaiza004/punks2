use crate::PlaybackError;
use rubato::{FftFixedIn, Resampler};

/// Resample interleaved f32 audio from `source_rate` to `target_rate`.
///
/// Input and output are interleaved with `channels` channels per frame.
/// Uses FFT-based synchronous resampling for quality and speed on pre-decoded buffers.
pub fn resample(
    interleaved: &[f32],
    channels: usize,
    source_rate: u32,
    target_rate: u32,
) -> Result<Vec<f32>, PlaybackError> {
    if channels == 0 {
        return Ok(Vec::new());
    }

    let num_frames = interleaved.len() / channels;
    if num_frames == 0 {
        return Ok(Vec::new());
    }

    let chunk_size = 1024.min(num_frames);

    let mut resampler = FftFixedIn::<f32>::new(
        source_rate as usize,
        target_rate as usize,
        chunk_size,
        2,
        channels,
    )
    .map_err(|e| PlaybackError::DecodeError(format!("resampler init: {e}")))?;

    // De-interleave into per-channel vectors
    let mut channel_data: Vec<Vec<f32>> = vec![Vec::with_capacity(num_frames); channels];
    for frame in 0..num_frames {
        for ch in 0..channels {
            channel_data[ch].push(interleaved[frame * channels + ch]);
        }
    }

    let mut output_channels: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut pos = 0;

    // Process full chunks
    while pos + chunk_size <= num_frames {
        let input: Vec<&[f32]> = channel_data
            .iter()
            .map(|ch| &ch[pos..pos + chunk_size])
            .collect();

        let out = resampler
            .process(&input, None)
            .map_err(|e| PlaybackError::DecodeError(format!("resample: {e}")))?;

        for (ch, data) in out.iter().enumerate() {
            output_channels[ch].extend_from_slice(data);
        }

        pos += chunk_size;
    }

    // Process remaining frames
    if pos < num_frames {
        let input: Vec<&[f32]> = channel_data.iter().map(|ch| &ch[pos..]).collect();

        let out = resampler
            .process_partial(Some(&input), None)
            .map_err(|e| PlaybackError::DecodeError(format!("resample partial: {e}")))?;

        for (ch, data) in out.iter().enumerate() {
            output_channels[ch].extend_from_slice(data);
        }
    }

    // Re-interleave
    let out_frames = output_channels[0].len();
    let mut result = Vec::with_capacity(out_frames * channels);
    for frame in 0..out_frames {
        for ch in &output_channels {
            result.push(ch[frame]);
        }
    }

    Ok(result)
}
