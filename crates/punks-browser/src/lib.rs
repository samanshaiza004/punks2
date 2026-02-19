use std::fmt;
use std::path::{Path, PathBuf};

pub use punks_core::{FileEntry, ScanResult, SUPPORTED_EXTENSIONS};
pub use punks_playback::{PlaybackError, PlaybackStatus};

use punks_core::ScanError;
use punks_playback::PlaybackEngine;

#[derive(Debug)]
pub enum BrowserError {
    Scan(ScanError),
    Playback(PlaybackError),
    NoSelection,
}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrowserError::Scan(e) => write!(f, "scan error: {e}"),
            BrowserError::Playback(e) => write!(f, "playback error: {e}"),
            BrowserError::NoSelection => write!(f, "no file selected"),
        }
    }
}

impl std::error::Error for BrowserError {}

impl From<ScanError> for BrowserError {
    fn from(e: ScanError) -> Self {
        BrowserError::Scan(e)
    }
}

impl From<PlaybackError> for BrowserError {
    fn from(e: PlaybackError) -> Self {
        BrowserError::Playback(e)
    }
}

/// The embeddable sample browser module.
///
/// Owns all state: file list, playback engine, current selection.
/// No global state — create one instance per browser panel.
pub struct SampleBrowser {
    scan_result: Option<ScanResult>,
    playback: PlaybackEngine,
    current_dir: Option<PathBuf>,
    selected: Option<usize>,
    last_error: Option<String>,
}

impl SampleBrowser {
    pub fn new() -> Result<Self, BrowserError> {
        let playback = PlaybackEngine::new()?;
        Ok(SampleBrowser {
            scan_result: None,
            playback,
            current_dir: None,
            selected: None,
            last_error: None,
        })
    }

    /// Call once per frame. Checks whether a background decode has finished
    /// and, if so, commits the audio buffer to start playback. Also picks up
    /// any decode errors and stores them in [`last_error`].
    pub fn poll(&mut self) {
        if let Some(err) = self.playback.poll() {
            self.last_error = Some(err.to_string());
        }
    }

    /// Scan a directory for audio files and replace the current file list.
    pub fn open_directory(&mut self, path: &Path) -> Result<(), BrowserError> {
        let result = punks_core::scan_directory(path, SUPPORTED_EXTENSIONS)?;
        self.current_dir = Some(path.to_path_buf());
        self.scan_result = Some(result);
        self.selected = None;
        self.last_error = None;
        Ok(())
    }

    pub fn files(&self) -> &[FileEntry] {
        self.scan_result
            .as_ref()
            .map(|r| r.files.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_directory(&self) -> Option<&Path> {
        self.current_dir.as_deref()
    }

    pub fn select(&mut self, index: usize) {
        if index < self.files().len() {
            self.selected = Some(index);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Begin playing the currently selected file. Decoding happens in the
    /// background — call [`poll`] each frame to complete the handoff.
    pub fn play_selected(&mut self) {
        let index = match self.selected {
            Some(i) => i,
            None => return,
        };
        let path = match self.files().get(index) {
            Some(entry) => entry.path.clone(),
            None => return,
        };

        self.last_error = None;
        self.playback.play(&path);
    }

    pub fn stop(&mut self) {
        self.playback.stop();
    }

    pub fn playback_status(&self) -> PlaybackStatus {
        self.playback.status()
    }

    /// Last error message, if any. Cleared on successful operations.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }
}
