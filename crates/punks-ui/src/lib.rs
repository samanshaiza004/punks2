use punks_browser::{PlaybackStatus, SampleBrowser};

/// UI-only state for the browser panel. Holds transient rendering state
/// that doesn't belong in the core SampleBrowser.
pub struct BrowserPanel {
    last_clicked: Option<usize>,
}

impl BrowserPanel {
    pub fn new() -> Self {
        BrowserPanel { last_clicked: None }
    }

    /// Draw the sample browser panel into the current imgui frame.
    ///
    /// Calls `browser.poll()` internally so the host does not need to
    /// remember a separate tick call.
    pub fn draw(&mut self, ui: &imgui::Ui, browser: &mut SampleBrowser) {
        browser.poll();

        // --- Directory picker ---
        if ui.button("Browse...") {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(e) = browser.open_directory(&path) {
                    log::error!("failed to open directory: {e}");
                }
            }
        }

        if let Some(dir) = browser.current_directory() {
            ui.same_line();
            ui.text(dir.display().to_string());
        }

        ui.separator();

        // --- File list ---
        let file_count = browser.files().len();
        let file_labels: Vec<(String, usize)> = browser
            .files()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let size_kb = entry.size_bytes as f64 / 1024.0;
                (format!("{}  ({:.1} KB)##file{}", entry.name, size_kb, i), i)
            })
            .collect();

        let avail = ui.content_region_avail();
        let list_height = (avail[1] - 60.0).max(100.0);

        ui.child_window("file_list")
            .size([avail[0], list_height])
            .build(|| {
                if file_count == 0 {
                    ui.text_disabled("No audio files. Click Browse to open a folder.");
                } else {
                    let selected = browser.selected();
                    for (label, i) in &file_labels {
                        let is_selected = selected == Some(*i);

                        if ui.selectable_config(label).selected(is_selected).build() {
                            browser.select(*i);
                            self.last_clicked = Some(*i);
                            browser.play_selected();
                        }
                    }
                }
            });

        ui.separator();

        // --- Playback status + loading indicator ---
        match browser.playback_status() {
            PlaybackStatus::Idle => {
                ui.text("Idle");
            }
            PlaybackStatus::Loading { file } => {
                let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                ui.text(format!("Loading: {name}..."));
            }
            PlaybackStatus::Playing {
                file,
                position,
                duration,
            } => {
                let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let pos_s = position.as_secs();
                let dur_s = duration.as_secs();
                ui.text(format!(
                    "Playing: {}  {}:{:02} / {}:{:02}",
                    name,
                    pos_s / 60,
                    pos_s % 60,
                    dur_s / 60,
                    dur_s % 60,
                ));
            }
        }

        // --- Stop button ---
        if ui.button("Stop") {
            browser.stop();
        }

        // --- Error display ---
        if let Some(err) = browser.last_error() {
            ui.same_line();
            ui.text_colored([1.0, 0.3, 0.3, 1.0], err);
        }
    }
}

impl Default for BrowserPanel {
    fn default() -> Self {
        Self::new()
    }
}
