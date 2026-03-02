use std::path::{Path, PathBuf};

use imgui::Key;
use punks_browser::{PlaybackStatus, SampleBrowser};
use punks_core::config::{Keybinds, PunksConfig};

#[derive(Clone, Copy, PartialEq)]
enum BrowserAction {
    NavigateUp,
    NavigateDown,
    NavigateBack,
    Confirm,
}

const CAPTURABLE_KEYS: &[(Key, &str)] = &[
    (Key::A, "A"),
    (Key::B, "B"),
    (Key::C, "C"),
    (Key::D, "D"),
    (Key::E, "E"),
    (Key::F, "F"),
    (Key::G, "G"),
    (Key::H, "H"),
    (Key::I, "I"),
    (Key::J, "J"),
    (Key::K, "K"),
    (Key::L, "L"),
    (Key::M, "M"),
    (Key::N, "N"),
    (Key::O, "O"),
    (Key::P, "P"),
    (Key::Q, "Q"),
    (Key::R, "R"),
    (Key::S, "S"),
    (Key::T, "T"),
    (Key::U, "U"),
    (Key::V, "V"),
    (Key::W, "W"),
    (Key::X, "X"),
    (Key::Y, "Y"),
    (Key::Z, "Z"),
    (Key::Enter, "Enter"),
    (Key::Space, "Space"),
    (Key::UpArrow, "UpArrow"),
    (Key::DownArrow, "DownArrow"),
    (Key::LeftArrow, "LeftArrow"),
    (Key::RightArrow, "RightArrow"),
    (Key::Tab, "Tab"),
];

fn parse_key(s: &str) -> Option<Key> {
    CAPTURABLE_KEYS
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(s))
        .map(|(k, _)| *k)
}

fn key_name(k: Key) -> &'static str {
    CAPTURABLE_KEYS
        .iter()
        .find(|(key, _)| *key == k)
        .map(|(_, name)| *name)
        .unwrap_or("?")
}

fn keybind_field_mut(keybinds: &mut Keybinds, action: BrowserAction) -> &mut String {
    match action {
        BrowserAction::NavigateUp => &mut keybinds.navigate_up,
        BrowserAction::NavigateDown => &mut keybinds.navigate_down,
        BrowserAction::NavigateBack => &mut keybinds.navigate_back,
        BrowserAction::Confirm => &mut keybinds.confirm,
    }
}

fn keybind_field(keybinds: &Keybinds, action: BrowserAction) -> &str {
    match action {
        BrowserAction::NavigateUp => &keybinds.navigate_up,
        BrowserAction::NavigateDown => &keybinds.navigate_down,
        BrowserAction::NavigateBack => &keybinds.navigate_back,
        BrowserAction::Confirm => &keybinds.confirm,
    }
}

const KEYBIND_ACTIONS: &[(BrowserAction, &str)] = &[
    (BrowserAction::NavigateUp, "Navigate up"),
    (BrowserAction::NavigateDown, "Navigate down"),
    (BrowserAction::NavigateBack, "Back"),
    (BrowserAction::Confirm, "Confirm / Play"),
];

pub struct BrowserPanel {
    prefs: PunksConfig,
    rebinding: Option<BrowserAction>,
}

impl BrowserPanel {
    pub fn new() -> Self {
        BrowserPanel {
            prefs: punks_core::config::load(),
            rebinding: None,
        }
    }

    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        on_drag_file: Option<&mut dyn FnMut(&Path)>,
    ) {
        browser.poll();

        if ui.button("Browse...") {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(e) = browser.open_directory(&path) {
                    log::error!("failed to open directory: {e}");
                } else {
                    self.prefs.last_directory = Some(path);
                    punks_core::config::save(&self.prefs);
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

        ui.same_line();
        if ui.button("Settings") {
            ui.open_popup("Settings##modal");
        }

        self.draw_settings_modal(ui, browser);

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
        let entry_meta: Vec<(String, bool, usize, PathBuf)> = browser
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
                (label, e.is_directory, i, e.path.clone())
            })
            .collect();

        let avail = ui.content_region_avail();
        let list_height = (avail[1] - 70.0).max(100.0);
        let mut drag_requested: Option<PathBuf> = None;

        let up_key = parse_key(&self.prefs.keybinds.navigate_up).unwrap_or(Key::W);
        let down_key = parse_key(&self.prefs.keybinds.navigate_down).unwrap_or(Key::S);
        let back_key = parse_key(&self.prefs.keybinds.navigate_back).unwrap_or(Key::A);
        let conf_key = parse_key(&self.prefs.keybinds.confirm).unwrap_or(Key::D);

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
                        if ui.is_key_pressed_no_repeat(up_key) {
                            let idx = selected.unwrap_or(0).saturating_sub(1);
                            browser.select(idx);
                            browser.play_selected();
                        }
                        if ui.is_key_pressed_no_repeat(down_key) {
                            let idx =
                                (selected.unwrap_or(0) + 1).min(entry_count.saturating_sub(1));
                            browser.select(idx);
                            browser.play_selected();
                        }
                        if ui.is_key_pressed_no_repeat(back_key) {
                            if let Err(e) = browser.navigate_up() {
                                log::error!("navigate_up failed: {e}");
                            }
                        }
                        let confirm = ui.is_key_pressed_no_repeat(conf_key)
                            || ui.is_key_pressed_no_repeat(Key::Enter)
                            || ui.is_key_pressed_no_repeat(Key::KeypadEnter);
                        if confirm {
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
                    for (label, is_dir, i, entry_path) in &entry_meta {
                        let is_selected = selected == Some(*i);
                        let display_label = label.split("##").next().unwrap_or(label);
                        let (clicked, used) = if *is_dir {
                            let color = ui.push_style_color(
                                imgui::StyleColor::Text,
                                [0.55, 0.85, 1.0, 1.0],
                            );
                            let clicked = ui
                                .selectable_config(display_label)
                                .selected(is_selected)
                                .build();
                            color.pop();
                            (clicked, true)
                        } else {
                            (
                                ui.selectable_config(label).selected(is_selected).build(),
                                true,
                            )
                        };
                        if !*is_dir
                            && ui.is_item_hovered()
                            && ui.is_mouse_dragging_with_threshold(imgui::MouseButton::Left, -1.0)
                        {
                            drag_requested = Some(entry_path.clone());
                            break;
                        }
                        if clicked && used {
                            browser.select(*i);
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

        if let Some(path) = drag_requested.as_deref() {
            if let Some(on_drag_file) = on_drag_file {
                on_drag_file(path);
            }
            return;
        }

        ui.separator();

        draw_waveform_widget(ui, browser);

        if ui.button("Stop") {
            browser.stop();
        }

        if let Some(err) = browser.last_error() {
            ui.same_line();
            ui.text_colored([1.0, 0.3, 0.3, 1.0], err);
        }
    }

    fn draw_settings_modal(&mut self, ui: &imgui::Ui, browser: &mut SampleBrowser) {
        let modal = ui
            .modal_popup_config("Settings##modal")
            .save_settings(false)
            .always_auto_resize(true);

        if let Some(_token) = modal.begin_popup() {
            ui.text("Samples folder");
            let dir_label = self
                .prefs
                .last_directory
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "(none)".into());
            ui.text_disabled(&dir_label);
            ui.same_line();
            if ui.button("Browse##settings") {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    if let Err(e) = browser.open_directory(&path) {
                        log::error!("failed to open directory: {e}");
                    } else {
                        self.prefs.last_directory = Some(path);
                        punks_core::config::save(&self.prefs);
                    }
                }
            }

            ui.separator();
            ui.text("Keybinds");
            ui.spacing();

            for &(action, label) in KEYBIND_ACTIONS {
                let is_rebinding = self.rebinding == Some(action);
                let current = keybind_field(&self.prefs.keybinds, action);
                let btn_label = if is_rebinding {
                    format!("Press any key...##{label}")
                } else {
                    let display = parse_key(current).map(key_name).unwrap_or(current);
                    format!("[ {display} ]##{label}")
                };

                ui.text(label);
                ui.same_line_with_pos(180.0);
                if ui.button(&btn_label) && !is_rebinding {
                    self.rebinding = Some(action);
                }
            }

            if let Some(action) = self.rebinding {
                for &(key, name) in CAPTURABLE_KEYS {
                    if ui.is_key_pressed_no_repeat(key) {
                        *keybind_field_mut(&mut self.prefs.keybinds, action) = name.to_string();
                        self.rebinding = None;
                        punks_core::config::save(&self.prefs);
                        break;
                    }
                }
            }

            ui.separator();
            if ui.button("Reset to defaults") {
                self.prefs.keybinds = Keybinds::default();
                punks_core::config::save(&self.prefs);
            }
            ui.same_line();
            if ui.button("Close") {
                self.rebinding = None;
                ui.close_current_popup();
            }
        }
    }
}

impl Default for BrowserPanel {
    fn default() -> Self {
        Self::new()
    }
}

const WAVEFORM_BG: [f32; 4] = [0.12, 0.12, 0.14, 1.0];
const WAVEFORM_BAR: [f32; 4] = [0.30, 0.75, 0.45, 1.0];
const WAVEFORM_PLAYHEAD: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const WAVEFORM_TEXT: [f32; 4] = [1.0, 1.0, 1.0, 0.85];

fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0) as u32;
    let g = (c[1] * 255.0) as u32;
    let b = (c[2] * 255.0) as u32;
    let a = (c[3] * 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

fn draw_waveform_widget(ui: &imgui::Ui, browser: &SampleBrowser) {
    let [cx, cy] = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0];
    const H: f32 = 64.0;
    ui.dummy([w, H]);

    let draw = ui.get_window_draw_list();

    let bg = color_u32(WAVEFORM_BG);
    let bar_color = color_u32(WAVEFORM_BAR);
    let playhead_color = color_u32(WAVEFORM_PLAYHEAD);
    let text_color = color_u32(WAVEFORM_TEXT);

    draw.add_rect([cx, cy], [cx + w, cy + H], bg)
        .filled(true)
        .build();

    if let Some(peaks) = browser.waveform_peaks() {
        let bar_w = (w / peaks.num_buckets as f32).max(1.0);
        let mid_y = cy + H / 2.0;
        let half_h = H / 2.0;

        for (i, &(lo, hi)) in peaks.peaks.iter().enumerate() {
            let x = cx + i as f32 * bar_w;
            let y_top = mid_y - hi * half_h;
            let y_bot = (mid_y - lo * half_h).max(y_top + 1.0);
            draw.add_rect([x, y_top], [x + bar_w - 0.5, y_bot], bar_color)
                .filled(true)
                .build();
        }
    }

    match browser.playback_status() {
        PlaybackStatus::Playing {
            file,
            position,
            duration,
        } => {
            let dur_secs = duration.as_secs_f32();
            if dur_secs > 0.0 {
                let t = position.as_secs_f32() / dur_secs;
                let px = cx + t * w;
                draw.add_line([px, cy], [px, cy + H], playhead_color)
                    .build();
            }
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let pos_s = position.as_secs();
            let dur_s = duration.as_secs();
            draw.add_text(
                [cx + 4.0, cy + 2.0],
                text_color,
                format!(
                    "{}  {}:{:02} / {}:{:02}",
                    name,
                    pos_s / 60,
                    pos_s % 60,
                    dur_s / 60,
                    dur_s % 60,
                ),
            );
        }
        PlaybackStatus::Loading { file } => {
            let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            draw.add_text(
                [cx + 4.0, cy + H / 2.0 - 7.0],
                text_color,
                format!("Loading: {name}..."),
            );
        }
        PlaybackStatus::Idle => {
            if browser.waveform_peaks().is_none() {
                draw.add_text(
                    [cx + 4.0, cy + H / 2.0 - 7.0],
                    color_u32([0.5, 0.5, 0.5, 0.7]),
                    "Idle",
                );
            }
        }
    }
}
