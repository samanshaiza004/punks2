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

/// One tab's navigation context: its own directory history, selection, and
/// search. Playback is global and lives on `SampleBrowser`, not here.
#[derive(Default)]
struct TabState {
    history: Vec<PathBuf>,
    listing: Option<DirListing>,
    selected: Option<usize>,
    /// Committed search text, so a tab restores its query when reactivated.
    search_query: String,
    search_results: Option<Vec<FileEntry>>,
    search_rx: Option<mpsc::Receiver<Vec<FileEntry>>>,
    search_selected: Option<usize>,
}

pub struct SampleBrowser {
    tabs: Vec<TabState>,
    active_tab: usize,
    playback: PlaybackEngine,
    last_error: Option<String>,
}

impl SampleBrowser {
    pub fn new() -> Result<Self, BrowserError> {
        let playback = PlaybackEngine::new()?;
        let mut browser = SampleBrowser {
            tabs: vec![TabState::default()],
            active_tab: 0,
            playback,
            last_error: None,
        };

        let cfg = punks_core::config::load();
        browser.playback.set_volume(cfg.volume);
        if let Some(dir) = cfg.last_directory.filter(|p| p.is_dir()) {
            let _ = browser.open_directory(&dir);
        }

        Ok(browser)
    }

    fn active(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn active_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    pub fn poll(&mut self) {
        if let Some(err) = self.playback.poll() {
            self.last_error = Some(err.to_string());
        }

        // Drain every tab's search channel, not just the active one, so a
        // search started in a tab still resolves while another tab is focused.
        for tab in &mut self.tabs {
            if let Some(rx) = &tab.search_rx {
                match rx.try_recv() {
                    Ok(results) => {
                        tab.search_results = Some(results);
                        tab.search_rx = None;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        tab.search_results = Some(Vec::new());
                        tab.search_rx = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
    }

    pub fn open_directory(&mut self, path: &Path) -> Result<(), BrowserError> {
        let listing = punks_core::list_directory(path)?;
        {
            let tab = self.active_mut();
            tab.history = vec![path.to_path_buf()];
            tab.listing = Some(listing);
            tab.selected = None;
        }
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
        let tab = self.active_mut();
        tab.history.push(path);
        tab.listing = Some(listing);
        tab.selected = None;
        Ok(())
    }

    pub fn navigate_up(&mut self) -> Result<(), BrowserError> {
        if self.active().history.len() <= 1 {
            return Ok(());
        }
        let path = {
            let tab = self.active_mut();
            tab.history.pop();
            tab.history.last().unwrap().clone()
        };
        let listing = punks_core::list_directory(&path)?;
        let tab = self.active_mut();
        tab.listing = Some(listing);
        tab.selected = None;
        Ok(())
    }

    pub fn navigate_to_breadcrumb(&mut self, level: usize) -> Result<(), BrowserError> {
        if level >= self.active().history.len() {
            return Ok(());
        }
        let path = {
            let tab = self.active_mut();
            tab.history.truncate(level + 1);
            tab.history.last().unwrap().clone()
        };
        let listing = punks_core::list_directory(&path)?;
        let tab = self.active_mut();
        tab.listing = Some(listing);
        tab.selected = None;
        Ok(())
    }

    pub fn entries(&self) -> &[FileEntry] {
        self.active()
            .listing
            .as_ref()
            .map(|l| l.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn current_directory(&self) -> Option<&Path> {
        self.active().history.last().map(PathBuf::as_path)
    }

    pub fn breadcrumbs(&self) -> Vec<String> {
        self.active()
            .history
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string_lossy().into_owned())
            })
            .collect()
    }
    pub fn can_navigate_up(&self) -> bool {
        self.active().history.len() > 1
    }

    pub fn select(&mut self, index: usize) {
        if index < self.entries().len() {
            self.active_mut().selected = Some(index);
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.active().selected
    }

    pub fn play_selected(&mut self) {
        let index = match self.active().selected {
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

    pub fn waveform_peaks(&self) -> Option<&WaveformPeaks> {
        self.playback.waveform_peaks()
    }

    pub fn set_volume(&self, v: f32) {
        self.playback.set_volume(v);
    }

    pub fn volume(&self) -> f32 {
        self.playback.volume()
    }

    pub fn search(&mut self, query: &str) {
        let root = match self.current_directory() {
            Some(p) => p.to_path_buf(),
            None => return,
        };
        let query = query.to_string();
        let (tx, rx) = mpsc::channel();
        let thread_query = query.clone();
        std::thread::spawn(move || {
            let results = punks_core::search_directory(&root, &thread_query, SUPPORTED_EXTENSIONS)
                .unwrap_or_else(|e| {
                    log::warn!("search in {}: {e}", root.display());
                    Vec::new()
                });
            let _ = tx.send(results);
        });
        let tab = self.active_mut();
        tab.search_rx = Some(rx);
        tab.search_results = None;
        tab.search_selected = None;
        tab.search_query = query;
    }

    pub fn clear_search(&mut self) {
        let tab = self.active_mut();
        tab.search_results = None;
        tab.search_rx = None;
        tab.search_selected = None;
        tab.search_query = String::new();
    }

    pub fn is_searching(&self) -> bool {
        self.active().search_rx.is_some()
    }

    pub fn is_in_search_mode(&self) -> bool {
        self.active().search_results.is_some() || self.active().search_rx.is_some()
    }

    pub fn search_results(&self) -> Option<&[FileEntry]> {
        self.active().search_results.as_deref()
    }

    pub fn search_selected(&self) -> Option<usize> {
        self.active().search_selected
    }

    pub fn select_search_result(&mut self, index: usize) {
        let valid = self
            .active()
            .search_results
            .as_ref()
            .is_some_and(|r| index < r.len());
        if valid {
            self.active_mut().search_selected = Some(index);
        }
    }

    // --- Tab management ---------------------------------------------------

    /// Create a new tab and make it active. `start` selects its initial
    /// directory: `Some(dir)` opens that directory in the new tab, `None`
    /// leaves it blank. The caller owns the policy (clone current / blank /
    /// last-saved) so it can be made pref-driven later.
    pub fn new_tab(&mut self, start: Option<&Path>) {
        self.tabs.push(TabState::default());
        self.active_tab = self.tabs.len() - 1;
        if let Some(dir) = start {
            let _ = self.open_directory(dir);
        }
    }

    /// Close the tab at `index`. No-op if only one tab remains.
    pub fn close_tab(&mut self, index: usize) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        self.active_tab = adjust_active_after_close(self.active_tab, index, self.tabs.len());
    }

    /// Make `index` the active tab (no-op if out of range).
    pub fn switch_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    /// Move the tab at `from` to position `to`, keeping the same logical tab
    /// active. Used by drag-to-reorder.
    pub fn reorder_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active_tab = adjust_active_after_reorder(self.active_tab, from, to);
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab(&self) -> usize {
        self.active_tab
    }

    /// Title for the tab at `index`: its current directory's name, falling
    /// back to the full path, or "New Tab" when no folder is open.
    pub fn tab_title(&self, index: usize) -> String {
        match self.tabs.get(index).and_then(|t| t.history.last()) {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned()),
            None => "New Tab".to_string(),
        }
    }

    /// The active tab's committed search text (for the UI search box to
    /// restore on tab switch).
    pub fn search_query(&self) -> &str {
        &self.active().search_query
    }
}

/// Active-tab index after removing the tab at `removed`. `new_len` is the tab
/// count *after* removal (>= 1). Closing a tab left of the active one shifts it
/// down; closing the active tab focuses the tab that slid into its slot,
/// clamped to the last tab.
fn adjust_active_after_close(active: usize, removed: usize, new_len: usize) -> usize {
    if active > removed {
        active - 1
    } else if active == removed {
        active.min(new_len - 1)
    } else {
        active
    }
}

/// Active-tab index after moving a tab from `from` to `to`, keeping the same
/// logical tab focused.
fn adjust_active_after_reorder(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else {
        let mut a = active;
        if from < a {
            a -= 1;
        }
        if to <= a {
            a += 1;
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::{adjust_active_after_close, adjust_active_after_reorder};

    #[test]
    fn close_left_of_active_shifts_down() {
        // [0,1,2,3], active=2, close 0 -> [1,2,3], active follows to 1
        assert_eq!(adjust_active_after_close(2, 0, 3), 1);
    }

    #[test]
    fn close_right_of_active_keeps_index() {
        // active=1, close 3 -> active unchanged
        assert_eq!(adjust_active_after_close(1, 3, 3), 1);
    }

    #[test]
    fn close_active_focuses_right_neighbor() {
        // [0,1,2], active=1, close 1 -> [0,2], focus the tab now at index 1
        assert_eq!(adjust_active_after_close(1, 1, 2), 1);
    }

    #[test]
    fn close_active_last_clamps() {
        // [0,1,2], active=2, close 2 -> [0,1], clamp to last index 1
        assert_eq!(adjust_active_after_close(2, 2, 2), 1);
    }

    #[test]
    fn reorder_moves_active_with_it() {
        // active tab dragged from 2 to 0 -> active is now 0
        assert_eq!(adjust_active_after_reorder(2, 2, 0), 0);
    }

    #[test]
    fn reorder_non_active_left_to_right_past_active() {
        // [a,b,c,d], active=1(b), move a:0->3 => [b,c,d,a], b now at 0
        assert_eq!(adjust_active_after_reorder(1, 0, 3), 0);
    }

    #[test]
    fn reorder_non_active_right_to_left_before_active() {
        // [a,b,c,d], active=1(b), move d:3->0 => [d,a,b,c], b now at 2
        assert_eq!(adjust_active_after_reorder(1, 3, 0), 2);
    }
}
