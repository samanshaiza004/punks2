use std::io::Cursor;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::PlaybackError;

pub struct DecodedAudio {
    pub interleaved: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

/// WAVE format tag for Ogg Vorbis wrapped in a RIFF/WAVE container (the bytes
/// spell "Og"). Produced by the Vorbis ACM codec and some older sample-pack
/// tooling. symphonia's RIFF reader has no codec mapping for it and bails, but
/// the `data` chunk is a complete, standalone Ogg stream we can decode directly.
const WAVE_FORMAT_OGG_VORBIS: u16 = 0x674f;

pub fn decode_file(path: &Path) -> Result<DecodedAudio, PlaybackError> {
    let raw =
        std::fs::read(path).map_err(|e| PlaybackError::DecodeError(format!("{path:?}: {e}")))?;

    // Special case: Ogg Vorbis in a WAV container. Skip the RIFF layer and hand
    // the inner Ogg bitstream straight to the Ogg reader / Vorbis decoder.
    if let Some(ogg) = extract_ogg_in_wav(&raw) {
        let mss = MediaSourceStream::new(Box::new(Cursor::new(ogg)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        return decode_from_stream(mss, &hint);
    }

    let mss = MediaSourceStream::new(Box::new(Cursor::new(raw)), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    decode_from_stream(mss, &hint)
}

/// If `raw` is an Ogg-Vorbis-in-WAV file (format tag 0x674f), return the inner
/// Ogg stream from the `data` chunk. Returns `None` for any normal file.
fn extract_ogg_in_wav(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < 44 || &raw[0..4] != b"RIFF" || &raw[8..12] != b"WAVE" {
        return None;
    }

    let mut pos = 12;
    let mut fmt_tag: Option<u16> = None;

    while pos + 8 <= raw.len() {
        let chunk_id = &raw[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([raw[pos + 4], raw[pos + 5], raw[pos + 6], raw[pos + 7]]) as usize;
        let body = pos + 8;

        match chunk_id {
            b"fmt " if body + 2 <= raw.len() => {
                fmt_tag = Some(u16::from_le_bytes([raw[body], raw[body + 1]]));
            }
            b"data" => {
                if fmt_tag == Some(WAVE_FORMAT_OGG_VORBIS) {
                    let end = (body + chunk_size).min(raw.len());
                    let inner = &raw[body..end];
                    if inner.starts_with(b"OggS") {
                        return Some(inner.to_vec());
                    }
                }
                return None;
            }
            _ => {}
        }
        // Chunks are word-aligned: a pad byte follows an odd-sized chunk.
        pos = body + chunk_size + (chunk_size & 1);
    }
    None
}

/// Probe and fully decode an already-built stream into interleaved f32 samples.
/// Shared by the normal and Ogg-in-WAV paths so the decode loop lives once.
fn decode_from_stream(mss: MediaSourceStream, hint: &Hint) -> Result<DecodedAudio, PlaybackError> {
    let probed = symphonia::default::get_probe()
        .format(
            hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| PlaybackError::DecodeError(format!("unsupported audio format: {e}")))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| PlaybackError::DecodeError("no audio track found".into()))?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();

    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| PlaybackError::DecodeError("unknown sample rate".into()))?;

    let channels = codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| PlaybackError::DecodeError(format!("codec init failed: {e}")))?;

    let mut all_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                break;
            }
            Err(e) => return Err(PlaybackError::DecodeError(format!("packet read: {e}"))),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(e)) => {
                log::warn!("decode error (skipping packet): {e}");
                continue;
            }
            Err(e) => return Err(PlaybackError::DecodeError(format!("decode: {e}"))),
        };

        let spec = *decoded.spec();
        let num_frames = decoded.frames();

        if num_frames == 0 {
            continue;
        }

        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        all_samples.extend_from_slice(sample_buf.samples());
    }

    if all_samples.is_empty() {
        return Err(PlaybackError::DecodeError("no audio data decoded".into()));
    }

    Ok(DecodedAudio {
        interleaved: all_samples,
        channels,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_ogg_in_wav, WAVE_FORMAT_OGG_VORBIS};

    /// Build a minimal RIFF/WAVE with the given fmt tag and data-chunk body.
    fn wav(fmt_tag: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&0u32.to_le_bytes()); // riff size (unused by extractor)
        v.extend_from_slice(b"WAVE");
        // fmt chunk: 16-byte body, first u16 is the format tag.
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&fmt_tag.to_le_bytes());
        v.extend_from_slice(&[0u8; 14]);
        // data chunk
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn extracts_inner_ogg_stream() {
        let ogg = b"OggS\x00\x02 fake vorbis bitstream";
        let file = wav(WAVE_FORMAT_OGG_VORBIS, ogg);
        assert_eq!(extract_ogg_in_wav(&file).as_deref(), Some(&ogg[..]));
    }

    #[test]
    fn ignores_normal_pcm_wav() {
        // Format tag 0x0001 = PCM; not Ogg-in-WAV.
        let file = wav(0x0001, b"\x00\x01\x02\x03");
        assert_eq!(extract_ogg_in_wav(&file), None);
    }

    #[test]
    fn ignores_ogg_tag_without_ogg_magic() {
        // Right fmt tag but the data chunk isn't actually an Ogg stream.
        let file = wav(WAVE_FORMAT_OGG_VORBIS, b"not ogg data");
        assert_eq!(extract_ogg_in_wav(&file), None);
    }

    #[test]
    fn ignores_non_riff() {
        assert_eq!(extract_ogg_in_wav(b"OggS at the very start, no RIFF"), None);
    }
}
