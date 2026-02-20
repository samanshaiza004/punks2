use imgui::Key;
use punks_browser::{PlaybackStatus, SampleBrowser};

pub struct BrowserPanel {
    _last_clicked: Option<usize>,
}

impl BrowserPanel {
    pub fn new() -> Self {
        BrowserPanel {
            _last_clicked: None,
        }
    }

    pub fn draw(&mut self, ui: &imgui::Ui, browser: &mut SampleBrowser) {
        browser.poll();
        if ui.button("Browse...") {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(e) = browser.open_directory(&path) {
                    log::error!("failed to open directory: {e}");
                }
            }
        }

        if browser.can_navigate_up() {
            ui.same_line();
            if ui.button("^  Up") {
                if let Err(e) = browser.navigate_up() {
                    log::error!("navigate_up failed: {e}");
                }
            }
        }

        let crumbs = browser.breadcrumbs();
        if !crumbs.is_empty() {
            ui.separator();
            for (i, crumb) in crumbs.iter().enumerate() {
                if i > 0 {
                    ui.same_line();
                    ui.text_disabled(">");
                    ui.same_line();
                }
                if i < crumbs.len() - 1 {
                    if ui.small_button(format!("{}##crumb{}", crumb, i)) {
                        if let Err(e) = browser.navigate_to_breadcrumb(i) {
                            log::error!("breadcrumb nav failed: {e}");
                        }
                    }
                } else {
                    ui.text(crumb);
                }
            }
        }

        ui.separator();

        let entry_count = browser.entries().len();
        let entry_meta: Vec<(String, bool, usize)> = browser
            .entries()
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let label = if e.is_directory {
                    format!("> {}##entry{}", e.name, i)
                } else {
                    let kb = e.size_bytes as f64 / 1024.0;
                    format!("{}  ({:.1} KB)##entry{}", e.name, kb, i)
                };
                (label, e.is_directory, i)
            })
            .collect();

        let avail = ui.content_region_avail();
        let list_height = (avail[1] - 70.0).max(100.0);

        // Keyboard: Up/Down move selection, Enter opens dir or plays file when list has focus.
        ui.child_window("file_list")
            .size([avail[0], list_height])
            .build(|| {
                if entry_count == 0 {
                    if browser.current_directory().is_some() {
                        ui.text_disabled("Empty directory.");
                    } else {
                        ui.text_disabled("No folder open. Click Browse to get started.");
                    }
                } else {
                    let selected = browser.selected();
                    if ui.is_window_focused() {
                        if ui.is_key_pressed_no_repeat(Key::UpArrow) {
                            let idx = selected.unwrap_or(0).saturating_sub(1);
                            browser.select(idx);
                        }
                        if ui.is_key_pressed_no_repeat(Key::DownArrow) {
                            let idx = (selected.unwrap_or(0) + 1).min(entry_count.saturating_sub(1));
                            browser.select(idx);
                        }
                        let enter = ui.is_key_pressed_no_repeat(Key::Enter)
                            || ui.is_key_pressed_no_repeat(Key::KeypadEnter);
                        if enter {
                            if let Some(i) = selected {
                                let entries = browser.entries();
                                if let Some(entry) = entries.get(i) {
                                    if entry.is_directory {
                                        if let Err(e) = browser.navigate_into(i) {
                                            log::error!("navigate_into failed: {e}");
                                        }
                                    } else {
                                        browser.play_selected();
                                    }
                                }
                            }
                        }
                    }
                    for (label, is_dir, i) in &entry_meta {
                        let is_selected = selected == Some(*i);
                        let display_label = label.split("##").next().unwrap_or(label);
                        let (clicked, used) = if *is_dir {
                            // Directories: selectable with tint so hover/selected match files
                            let color =
                                ui.push_style_color(imgui::StyleColor::Text, [0.55, 0.85, 1.0, 1.0]);
                            let clicked = ui
                                .selectable_config(display_label)
                                .selected(is_selected)
                                .build();
                            color.pop();
                            (clicked, true)
                        } else {
                            (ui.selectable_config(label).selected(is_selected).build(), true)
                        };
                        if clicked && used {
                            browser.select(*i);
                            self._last_clicked = Some(*i);
                            if *is_dir {
                                if let Err(e) = browser.navigate_into(*i) {
                                    log::error!("navigate_into failed: {e}");
                                }
                            } else {
                                browser.play_selected();
                            }
                        }
                    }
                }
            });

        ui.separator();

        match browser.playback_status() {
            PlaybackStatus::Idle => {
                ui.text_disabled("Idle");
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

        if ui.button("Stop") {
            browser.stop();
        }

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
