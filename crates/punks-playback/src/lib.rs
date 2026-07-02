use std::fmt;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::Duration;

use lru::LruCache;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;

mod decode;
pub mod peaks;
mod resample;

pub use decode::AudioMetadata;
pub use peaks::WaveformPeaks;

/// Container-level info about the currently loaded track: free-text metadata,
/// its true source length, and whether only a preview window was decoded.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub metadata: AudioMetadata,
    pub source_sample_rate: u32,
    pub source_duration: Duration,
    pub preview_duration: Duration,
    pub truncated: bool,
}

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
    samples: RwLock<Vec<f32>>,
    cursor: AtomicUsize,
    playing: AtomicBool,
    total_frames: AtomicUsize,
    volume: AtomicU32,
}

#[derive(Clone)]
struct PreparedAudio {
    samples: Vec<f32>,
    total_frames: usize,
    file: PathBuf,
    peaks: WaveformPeaks,
    info: TrackInfo,
}

/// A "latest wins" single-slot mailbox: `send` replaces whatever is waiting
/// (if anything), `recv` blocks until a value is available. This coalesces
/// rapid decode requests into a single persistent worker thread — if several
/// `send`s land before the worker is free, only the most recent one is ever
/// received.
struct RequestSlot<T> {
    slot: Mutex<Option<T>>,
    cv: Condvar,
}

impl<T> RequestSlot<T> {
    fn new() -> Self {
        RequestSlot {
            slot: Mutex::new(None),
            cv: Condvar::new(),
        }
    }

    fn send(&self, value: T) {
        let mut guard = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(value);
        self.cv.notify_one();
    }

    fn recv(&self) -> T {
        let mut guard = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(value) = guard.take() {
                return value;
            }
            guard = self.cv.wait(guard).unwrap_or_else(|e| e.into_inner());
        }
    }
}

const CACHE_CAPACITY: usize = 10;

pub struct PlaybackEngine {
    shared: Arc<SharedState>,
    _stream: cpal::Stream,
    device_sample_rate: u32,
    device_channels: u16,
    current_file: Option<PathBuf>,
    current_peaks: Option<WaveformPeaks>,
    current_info: Option<TrackInfo>,
    /// File we're currently awaiting a decode for, if any.
    pending: Option<PathBuf>,
    cache: LruCache<PathBuf, Arc<PreparedAudio>>,
    decode_request: Arc<RequestSlot<PathBuf>>,
    decode_result_rx: mpsc::Receiver<(PathBuf, Result<PreparedAudio, PlaybackError>)>,
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
            volume: AtomicU32::new(1.0f32.to_bits()),
        });

        let cb_shared = Arc::clone(&shared);

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    audio_callback(data, &cb_shared);
                },
                |err| log::error!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| PlaybackError::DeviceError(e.to_string()))?;

        // One persistent decode worker for the engine's lifetime, instead of a
        // thread per play() call. Rapid navigation (holding W/S) now coalesces
        // into a single in-flight decode via RequestSlot rather than spawning
        // and fully decoding a thread per keypress.
        let decode_request = Arc::new(RequestSlot::<PathBuf>::new());
        let (result_tx, result_rx) = mpsc::channel();
        {
            let decode_request = Arc::clone(&decode_request);
            let target_channels = channels as usize;
            let target_rate = sample_rate;
            std::thread::spawn(move || loop {
                let path = decode_request.recv();
                let result = decode_and_prepare(&path, target_channels, target_rate);
                // ponytail: no explicit shutdown signal. If the engine is
                // dropped while this thread is between decodes (blocked in
                // recv()), the thread parks forever rather than exiting.
                // Acceptable because PlaybackEngine is owned by SampleBrowser
                // for the app's entire process lifetime today — process exit
                // reclaims it regardless. Upgrade path if that ever changes:
                // give RequestSlot a Shutdown variant the worker checks after
                // waking.
                if result_tx.send((path, result)).is_err() {
                    break; // receiver dropped; nothing left to report to.
                }
            });
        }

        Ok(PlaybackEngine {
            shared,
            _stream: stream,
            device_sample_rate: sample_rate,
            device_channels: channels,
            current_file: None,
            current_peaks: None,
            current_info: None,
            pending: None,
            cache: LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap()),
            decode_request,
            decode_result_rx: result_rx,
        })
    }

    fn commit(&mut self, audio: &Arc<PreparedAudio>) {
        {
            // Recover from a poisoned lock rather than propagating a panic to
            // the UI thread. The samples are the source of truth; if the lock
            // was poisoned mid-write the data is suspect, but silence from the
            // audio callback is safer than a crash.
            let mut buf = self
                .shared
                .samples
                .write()
                .unwrap_or_else(|e| e.into_inner());
            buf.clone_from(&audio.samples);
        }
        self.shared.cursor.store(0, Ordering::SeqCst);
        self.shared
            .total_frames
            .store(audio.total_frames, Ordering::SeqCst);
        self.current_file = Some(audio.file.clone());
        self.current_peaks = Some(audio.peaks.clone());
        self.current_info = Some(audio.info.clone());
        // Release pairs with the Acquire load in audio_callback, so the
        // callback is guaranteed to observe cursor=0 and the new samples
        // whenever it sees playing==true.
        self.shared.playing.store(true, Ordering::Release);
        self.pending = None;
    }

    /// Begin loading and playing a file. If the file was recently decoded it
    /// is served from an in-memory cache and playback starts immediately.
    /// Otherwise the request is handed to the persistent decode worker and
    /// this returns immediately. Call [`poll`] each frame to check for
    /// completion and commit the audio buffer.
    pub fn play(&mut self, path: &Path) {
        self.shared.playing.store(false, Ordering::SeqCst);

        let path_buf = path.to_path_buf();

        if let Some(cached) = self.cache.get(&path_buf) {
            let cached = Arc::clone(cached);
            self.pending = None;
            self.commit(&cached);
            return;
        }

        self.current_peaks = None;
        self.current_info = None;

        // If a decode is already in flight, this replaces the queued path —
        // RequestSlot coalesces to the latest — so rapid navigation collapses
        // into a single decode instead of spawning a thread per keypress.
        self.decode_request.send(path_buf.clone());
        self.pending = Some(path_buf);
    }

    pub fn poll(&mut self) -> Option<PlaybackError> {
        self.pending.as_ref()?;

        loop {
            match self.decode_result_rx.try_recv() {
                Ok((path, result)) => {
                    // A result for a request superseded by a later play() call
                    // (or abandoned for a cache hit) — discard and keep
                    // draining rather than returning it.
                    if self.pending.as_deref() != Some(path.as_path()) {
                        continue;
                    }
                    return match result {
                        Ok(audio) => {
                            let arc = Arc::new(audio);
                            // Previews of long files are large and re-auditioned
                            // rarely; keep them out of the cache so it stays
                            // full of small one-shots.
                            if !arc.info.truncated {
                                self.cache.put(arc.file.clone(), Arc::clone(&arc));
                            }
                            self.commit(&arc);
                            None
                        }
                        Err(e) => {
                            self.pending = None;
                            Some(e)
                        }
                    };
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Some(PlaybackError::DecodeError(
                        "decode worker terminated unexpectedly".into(),
                    ));
                }
            }
        }
    }

    pub fn stop(&mut self) {
        self.shared.playing.store(false, Ordering::SeqCst);
        self.pending = None;
        // Keep current_file / current_info (and the decoded buffer) so the clip
        // stays loaded and scrubbable after Stop — seek_fraction can resume it,
        // and status() correctly reports Playing once it does. A new play()
        // overwrites them, so nothing goes stale.
    }

    pub fn status(&self) -> PlaybackStatus {
        if let Some(file) = &self.pending {
            return PlaybackStatus::Loading { file: file.clone() };
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

    pub fn waveform_peaks(&self) -> Option<&WaveformPeaks> {
        self.current_peaks.as_ref()
    }

    pub fn current_info(&self) -> Option<&TrackInfo> {
        self.current_info.as_ref()
    }

    /// Playable duration of the currently loaded buffer (device rate), or
    /// `None` when nothing is loaded. Used by the UI to label scrub positions.
    pub fn loaded_duration(&self) -> Option<Duration> {
        let total = self.shared.total_frames.load(Ordering::Relaxed);
        (total > 0).then(|| Duration::from_secs_f64(total as f64 / self.device_sample_rate as f64))
    }

    /// Seek to `fraction` (0..1) of the loaded buffer and (re)start playback
    /// from there. No re-decode: the whole clip is already in `shared.samples`,
    /// so this just repositions the cursor.
    ///
    /// `&self` — atomics only, like `set_volume`. Ordering mirrors `commit`:
    /// the `cursor` store is published by the `Release` store of `playing`,
    /// which the audio callback loads with `Acquire`, so a seek from a stopped
    /// or finished clip never reads a stale position. Seeking mid-playback is
    /// also safe — `cursor` is a single coherent atomic and the callback
    /// re-checks bounds every buffer.
    ///
    /// Note: this operates on whatever is currently loaded. While a new clip is
    /// still decoding, that is the *previous* clip's buffer (poll/commit and
    /// this call are both on the main thread, so there is no race — just the
    /// previous buffer until the new one commits).
    pub fn seek_fraction(&self, fraction: f32) {
        let total = self.shared.total_frames.load(Ordering::Relaxed);
        if total == 0 {
            return;
        }
        let frame = frame_for_fraction(total, fraction);
        let channels = self.device_channels.max(1) as usize;
        self.shared.cursor.store(frame * channels, Ordering::SeqCst);
        self.shared.playing.store(true, Ordering::Release);
    }

    pub fn set_volume(&self, v: f32) {
        self.shared
            .volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }
}

fn decode_and_prepare(
    path: &Path,
    target_channels: usize,
    target_rate: u32,
) -> Result<PreparedAudio, PlaybackError> {
    let decoded = decode::decode_file(path)?;

    let waveform_peaks = peaks::compute_peaks(
        &decoded.interleaved,
        decoded.channels as usize,
        peaks::DEFAULT_NUM_BUCKETS,
    );

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

    let info = TrackInfo {
        source_sample_rate: decoded.sample_rate,
        source_duration: decoded.source_duration,
        preview_duration: decoded.preview_duration,
        truncated: decoded.truncated,
        metadata: decoded.metadata,
    };

    Ok(PreparedAudio {
        samples,
        total_frames,
        file: path.to_path_buf(),
        peaks: waveform_peaks,
        info,
    })
}

fn audio_callback(data: &mut [f32], shared: &SharedState) {
    // Acquire pairs with the Release store in commit(), ensuring this thread
    // sees cursor=0 and the new sample buffer whenever playing is true.
    if !shared.playing.load(Ordering::Acquire) {
        data.fill(0.0);
        return;
    }

    if let Ok(samples) = shared.samples.try_read() {
        let cursor = shared.cursor.load(Ordering::Relaxed);
        let remaining = samples.len().saturating_sub(cursor);
        let to_copy = remaining.min(data.len());
        let volume = f32::from_bits(shared.volume.load(Ordering::Relaxed));

        for (dst, &src) in data[..to_copy]
            .iter_mut()
            .zip(&samples[cursor..cursor + to_copy])
        {
            *dst = src * volume;
        }

        if to_copy < data.len() {
            data[to_copy..].fill(0.0);
            shared.playing.store(false, Ordering::Relaxed);
        }

        shared.cursor.store(cursor + to_copy, Ordering::Relaxed);
    } else {
        data.fill(0.0);
    }
}

/// Map a 0..1 scrub fraction to a frame index in a buffer of `total_frames`.
/// Clamps out-of-range fractions and never returns `>= total_frames`.
fn frame_for_fraction(total_frames: usize, fraction: f32) -> usize {
    if total_frames == 0 {
        return 0;
    }
    let f = fraction.clamp(0.0, 1.0);
    ((total_frames as f32 * f) as usize).min(total_frames - 1)
}

fn adapt_channels(samples: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return samples.to_vec();
    }

    let num_frames = samples.len() / from;
    let mut out = Vec::with_capacity(num_frames * to);
    let inv_from = 1.0 / from as f32;

    for frame in 0..num_frames {
        let base = frame * from;
        if from > to {
            // Downmix: sum all source channels to mono, then write to every
            // output channel. (L+R)/2 for stereo→mono; correct for all counts.
            let mono: f32 = (0..from).map(|ch| samples[base + ch]).sum::<f32>() * inv_from;
            for _ in 0..to {
                out.push(mono);
            }
        } else {
            // Upmix: copy available channels, replicate last one for the rest.
            for ch in 0..to {
                out.push(samples[base + ch.min(from - 1)]);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{frame_for_fraction, RequestSlot};
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn frame_for_fraction_maps_and_clamps() {
        assert_eq!(frame_for_fraction(1000, 0.0), 0);
        assert_eq!(frame_for_fraction(1000, 0.5), 500);
        assert_eq!(frame_for_fraction(1000, 1.0), 999); // never == total
        assert_eq!(frame_for_fraction(1000, -1.0), 0); // clamps low
        assert_eq!(frame_for_fraction(1000, 2.0), 999); // clamps high
        assert_eq!(frame_for_fraction(0, 0.5), 0); // empty buffer
    }

    #[test]
    fn request_slot_coalesces_to_latest() {
        // Several sends before anyone reads: only the last one should surface.
        let slot = RequestSlot::new();
        slot.send(1);
        slot.send(2);
        slot.send(3);
        assert_eq!(slot.recv(), 3);
    }

    #[test]
    fn request_slot_recv_blocks_until_send() {
        let slot = Arc::new(RequestSlot::new());
        let reader = Arc::clone(&slot);
        let handle = std::thread::spawn(move || reader.recv());
        std::thread::sleep(Duration::from_millis(20));
        slot.send(42);
        assert_eq!(handle.join().unwrap(), 42);
    }
}
