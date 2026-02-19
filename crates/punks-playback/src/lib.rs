use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;

mod decode;
mod resample;

#[derive(Debug, Clone)]
pub enum PlaybackStatus {
    Idle,
    Loading {
        file: PathBuf,
    },
    Playing {
        file: PathBuf,
        position: Duration,
        duration: Duration,
    },
}

#[derive(Debug)]
pub enum PlaybackError {
    DecodeError(String),
    DeviceError(String),
    UnsupportedFormat,
}

impl fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaybackError::DecodeError(e) => write!(f, "decode error: {e}"),
            PlaybackError::DeviceError(e) => write!(f, "device error: {e}"),
            PlaybackError::UnsupportedFormat => write!(f, "unsupported audio format"),
        }
    }
}

impl std::error::Error for PlaybackError {}

struct SharedState {
    /// Interleaved f32 samples at the device's sample rate and channel count.
    samples: RwLock<Vec<f32>>,
    /// Current read position in the samples buffer (in individual samples, not frames).
    cursor: AtomicUsize,
    playing: AtomicBool,
    /// Total number of frames (samples.len() / device_channels).
    total_frames: AtomicUsize,
}

struct PreparedAudio {
    samples: Vec<f32>,
    total_frames: usize,
    file: PathBuf,
}

struct PendingLoad {
    file: PathBuf,
    receiver: mpsc::Receiver<Result<PreparedAudio, PlaybackError>>,
}

pub struct PlaybackEngine {
    shared: Arc<SharedState>,
    _stream: cpal::Stream,
    device_sample_rate: u32,
    device_channels: u16,
    current_file: Option<PathBuf>,
    pending: Option<PendingLoad>,
}

impl PlaybackEngine {
    pub fn new() -> Result<Self, PlaybackError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PlaybackError::DeviceError("no output device found".into()))?;

        let supported_config = device
            .default_output_config()
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        let sample_rate = supported_config.sample_rate();
        let channels = supported_config.channels();

        let config: StreamConfig = supported_config.into();

        let shared = Arc::new(SharedState {
            samples: RwLock::new(Vec::new()),
            cursor: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            total_frames: AtomicUsize::new(0),
        });

        let cb_shared = Arc::clone(&shared);
        let cb_channels = channels as usize;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    audio_callback(data, &cb_shared, cb_channels);
                },
                |err| log::error!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        Ok(PlaybackEngine {
            shared,
            _stream: stream,
            device_sample_rate: sample_rate,
            device_channels: channels,
            current_file: None,
            pending: None,
        })
    }

    /// Begin loading and playing a file. Decoding and resampling happen on a
    /// background thread — this returns immediately. Call [`poll`] each frame
    /// to check for completion and commit the audio buffer.
    pub fn play(&mut self, path: &Path) {
        self.shared.playing.store(false, Ordering::SeqCst);
        // Drop any in-flight decode (the orphaned thread will finish and its
        // send will harmlessly fail on the disconnected channel).
        self.pending = None;

        let path_buf = path.to_path_buf();
        let target_channels = self.device_channels as usize;
        let target_rate = self.device_sample_rate;

        let (tx, rx) = mpsc::channel();

        let thread_path = path_buf.clone();
        std::thread::spawn(move || {
            let result = decode_and_prepare(&thread_path, target_channels, target_rate);
            let _ = tx.send(result);
        });

        self.pending = Some(PendingLoad {
            file: path_buf,
            receiver: rx,
        });
    }

    /// Poll for background decode completion. Call once per frame from the UI
    /// thread. Returns `Some(err)` if decoding failed, `None` otherwise.
    pub fn poll(&mut self) -> Option<PlaybackError> {
        let pending = self.pending.as_ref()?;

        match pending.receiver.try_recv() {
            Ok(Ok(audio)) => {
                {
                    let mut buf = self.shared.samples.write().unwrap();
                    *buf = audio.samples;
                }
                self.shared.cursor.store(0, Ordering::SeqCst);
                self.shared
                    .total_frames
                    .store(audio.total_frames, Ordering::SeqCst);
                self.current_file = Some(audio.file);
                self.shared.playing.store(true, Ordering::SeqCst);
                self.pending = None;
                None
            }
            Ok(Err(e)) => {
                self.pending = None;
                Some(e)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                Some(PlaybackError::DecodeError(
                    "decode thread terminated unexpectedly".into(),
                ))
            }
        }
    }

    pub fn stop(&mut self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.pending = None;
        self.current_file = None;
    }

    /// Returns `true` while a background decode is in progress.
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn status(&self) -> PlaybackStatus {
        if let Some(pending) = &self.pending {
            return PlaybackStatus::Loading {
                file: pending.file.clone(),
            };
        }

        if !self.shared.playing.load(Ordering::Relaxed) {
            return PlaybackStatus::Idle;
        }

        match &self.current_file {
            Some(file) => {
                let cursor = self.shared.cursor.load(Ordering::Relaxed);
                let total = self.shared.total_frames.load(Ordering::Relaxed);
                let channels = self.device_channels as usize;
                let frame = if channels > 0 { cursor / channels } else { 0 };
                let rate = self.device_sample_rate as f64;

                PlaybackStatus::Playing {
                    file: file.clone(),
                    position: Duration::from_secs_f64(frame as f64 / rate),
                    duration: Duration::from_secs_f64(total as f64 / rate),
                }
            }
            None => PlaybackStatus::Idle,
        }
    }
}

/// Decode, channel-adapt, and resample on the calling thread.
fn decode_and_prepare(
    path: &Path,
    target_channels: usize,
    target_rate: u32,
) -> Result<PreparedAudio, PlaybackError> {
    let decoded = decode::decode_file(path)?;

    let samples = adapt_channels(
        &decoded.interleaved,
        decoded.channels as usize,
        target_channels,
    );

    let samples = if decoded.sample_rate != target_rate {
        resample::resample(&samples, target_channels, decoded.sample_rate, target_rate)?
    } else {
        samples
    };

    let total_frames = samples.len() / target_channels;

    Ok(PreparedAudio {
        samples,
        total_frames,
        file: path.to_path_buf(),
    })
}

fn audio_callback(data: &mut [f32], shared: &SharedState, _channels: usize) {
    if !shared.playing.load(Ordering::Relaxed) {
        data.fill(0.0);
        return;
    }

    if let Ok(samples) = shared.samples.try_read() {
        let cursor = shared.cursor.load(Ordering::Relaxed);
        let remaining = samples.len().saturating_sub(cursor);
        let to_copy = remaining.min(data.len());

        data[..to_copy].copy_from_slice(&samples[cursor..cursor + to_copy]);

        if to_copy < data.len() {
            data[to_copy..].fill(0.0);
            shared.playing.store(false, Ordering::Relaxed);
        }

        shared.cursor.store(cursor + to_copy, Ordering::Relaxed);
    } else {
        data.fill(0.0);
    }
}

/// Convert interleaved audio between different channel counts.
fn adapt_channels(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return samples.to_vec();
    }

    let num_frames = samples.len() / from;
    let mut out = Vec::with_capacity(num_frames * to);

    for frame in 0..num_frames {
        let base = frame * from;
        for ch in 0..to {
            if ch < from {
                out.push(samples[base + ch]);
            } else {
                out.push(samples[base + from - 1]);
            }
        }
    }

    out
}
