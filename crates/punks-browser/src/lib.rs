use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub use punks_core::{DirListing, FileEntry, ScanError, SUPPORTED_EXTENSIONS};
pub use punks_playback::{PlaybackError, PlaybackStatus, WaveformPeaks};

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

pub struct SampleBrowser {
    history: Vec<PathBuf>,
    listing: Option<DirListing>,
    playback: PlaybackEngine,
    selected: Option<usize>,
    last_error: Option<String>,
    search_results: Option<Vec<FileEntry>>,
    search_rx: Option<mpsc::Receiver<Vec<FileEntry>>>,
    search_selected: Option<usize>,
}

impl SampleBrowser {
    pub fn new() -> Result<Self, BrowserError> {
        let playback = PlaybackEngine::new()?;
        let mut browser = SampleBrowser {
            history: Vec::new(),
            listing: None,
            playback,
            selected: None,
            last_error: None,
            search_results: None,
            search_rx: None,
            search_selected: None,
        };

        let cfg = punks_core::config::load();
        if let Some(dir) = cfg.last_directory.filter(|p| p.is_dir()) {
            let _ = browser.open_directory(&dir);
        }

        Ok(browser)
    }

    pub fn poll(&mut self) {
        if let Some(err) = self.playback.poll() {
            self.last_error = Some(err.to_string());
        }

        if let Some(rx) = &self.search_rx {
            match rx.try_recv() {
                Ok(results) => {
                    self.search_results = Some(results);
                    self.search_rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.search_results = Some(Vec::new());
                    self.search_rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub fn open_directory(&mut self, path: &Path) -> Result<(), BrowserError> {
        let listing = punks_core::list_directory(path)?;
        self.history = vec![path.to_path_buf()];
        self.listing = Some(listing);
        self.selected = None;
        self.last_error = None;
        self.clear_search();
        Ok(())
    }

    pub fn navigate_into(&mut self, index: usize) -> Result<(), BrowserError> {
        let path = {
            let entry = self.entries().get(index).ok_or(BrowserError::NoSelection)?;
            if !entry.is_directory {
                return Err(BrowserError::NoSelection);
            }
            entry.path.clone()
        };

        let listing = punks_core::list_directory(&path)?;
        self.history.push(path);
        self.listing = Some(listing);
        self.selected = None;
        Ok(())
    }

    pub fn navigate_up(&mut self) -> Result<(), BrowserError> {
        if self.history.len() <= 1 {
            return Ok(());
        }
        self.history.pop();
        let path = self.history.last().unwrap().clone();
        let listing = punks_core::list_directory(&path)?;
        self.listing = Some(listing);
        self.selected = None;
        Ok(())
    }

    pub fn navigate_to_breadcrumb(&mut self, level: usize) -> Result<(), BrowserError> {
        if level >= self.history.len() {
            return Ok(());
        }
        self.history.truncate(level + 1);
        let path = self.history.last().unwrap().clone();
        let listing = punks_core::list_directory(&path)?;
        self.listing = Some(listing);
        self.selected = None;
        Ok(())
    }

    pub fn entries(&self) -> &[FileEntry] {
        self.listing
            .as_ref()
            .map(|l| l.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_directory(&self) -> Option<&Path> {
        self.history.last().map(PathBuf::as_path)
    }

    pub fn breadcrumbs(&self) -> Vec<String> {
        self.history
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string_lossy().into_owned())
            })
            .collect()
    }
    pub fn can_navigate_up(&self) -> bool {
        self.history.len() > 1
    }

    pub fn select(&mut self, index: usize) {
        if index < self.entries().len() {
            self.selected = Some(index);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn play_selected(&mut self) {
        let index = match self.selected {
            Some(i) => i,
            None => return,
        };
        let path = match self.entries().get(index) {
            Some(entry) if !entry.is_directory => entry.path.clone(),
            _ => return,
        };

        self.last_error = None;
        self.playback.play(&path);
    }

    pub fn play_file(&mut self, path: &Path) {
        self.last_error = None;
        self.playback.play(path);
    }

    pub fn stop(&mut self) {
        self.playback.stop();
    }

    pub fn playback_status(&self) -> PlaybackStatus {
        self.playback.status()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    pub fn waveform_peaks(&self) -> Option<&WaveformPeaks> {
        self.playback.waveform_peaks()
    }

    pub fn search(&mut self, query: &str) {
        let root = match self.current_directory() {
            Some(p) => p.to_path_buf(),
            None => return,
        };
        let query = query.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let results = punks_core::search_directory(&root, &query, SUPPORTED_EXTENSIONS)
                .unwrap_or_default();
            let _ = tx.send(results);
        });
        self.search_rx = Some(rx);
        self.search_results = None;
        self.search_selected = None;
    }

    pub fn clear_search(&mut self) {
        self.search_results = None;
        self.search_rx = None;
        self.search_selected = None;
    }

    pub fn is_searching(&self) -> bool {
        self.search_rx.is_some()
    }

    pub fn is_in_search_mode(&self) -> bool {
        self.search_results.is_some() || self.search_rx.is_some()
    }

    pub fn search_results(&self) -> Option<&[FileEntry]> {
        self.search_results.as_deref()
    }

    pub fn search_selected(&self) -> Option<usize> {
        self.search_selected
    }

    pub fn select_search_result(&mut self, index: usize) {
        if let Some(results) = &self.search_results {
            if index < results.len() {
                self.search_selected = Some(index);
            }
        }
    }
}
