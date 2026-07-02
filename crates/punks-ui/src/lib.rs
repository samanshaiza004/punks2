use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use imgui::Key;
use punks_browser::{PlaybackStatus, SampleBrowser};
use punks_core::config::{Keybinds, PunksConfig};

#[derive(Clone, Copy, PartialEq)]
enum BrowserAction {
    NavigateUp,
    NavigateDown,
    NavigateBack,
    Confirm,
    NewTab,
    CloseTab,
    PrevTab,
    NextTab,
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
        BrowserAction::NewTab => &mut keybinds.new_tab,
        BrowserAction::CloseTab => &mut keybinds.close_tab,
        BrowserAction::PrevTab => &mut keybinds.prev_tab,
        BrowserAction::NextTab => &mut keybinds.next_tab,
    }
}

fn keybind_field(keybinds: &Keybinds, action: BrowserAction) -> &str {
    match action {
        BrowserAction::NavigateUp => &keybinds.navigate_up,
        BrowserAction::NavigateDown => &keybinds.navigate_down,
        BrowserAction::NavigateBack => &keybinds.navigate_back,
        BrowserAction::Confirm => &keybinds.confirm,
        BrowserAction::NewTab => &keybinds.new_tab,
        BrowserAction::CloseTab => &keybinds.close_tab,
        BrowserAction::PrevTab => &keybinds.prev_tab,
        BrowserAction::NextTab => &keybinds.next_tab,
    }
}

const KEYBIND_ACTIONS: &[(BrowserAction, &str)] = &[
    (BrowserAction::NavigateUp, "Navigate up"),
    (BrowserAction::NavigateDown, "Navigate down"),
    (BrowserAction::NavigateBack, "Back"),
    (BrowserAction::Confirm, "Confirm / Play"),
    (BrowserAction::NewTab, "New tab"),
    (BrowserAction::CloseTab, "Close tab"),
    (BrowserAction::PrevTab, "Previous tab"),
    (BrowserAction::NextTab, "Next tab"),
];

// Tab palette: active tab carries a muted blue accent, inactive tabs are grey.
const TAB_ACTIVE_BG: [f32; 4] = [0.24, 0.36, 0.52, 1.0];
const TAB_ACTIVE_HOVER: [f32; 4] = [0.28, 0.41, 0.59, 1.0];
const TAB_INACTIVE_BG: [f32; 4] = [0.16, 0.17, 0.20, 1.0];
const TAB_INACTIVE_HOVER: [f32; 4] = [0.22, 0.23, 0.27, 1.0];
const TAB_CLOSE_TEXT: [f32; 4] = [0.70, 0.72, 0.76, 1.0];

const DIR_TEXT_COLOR: [f32; 4] = [0.55, 0.85, 1.0, 1.0];

// File list lays out entries in width-adaptive columns; each column is at least
// this wide, so wide windows show 2+ columns and narrow ones collapse to 1.
const MIN_COLUMN_WIDTH: f32 = 300.0;
const COLUMN_GUTTER: f32 = 8.0;

fn column_count(avail_width: f32) -> usize {
    ((avail_width / MIN_COLUMN_WIDTH).floor() as usize).max(1)
}

/// Duration as `M:SS` (or `H:MM:SS` past an hour).
fn format_hms(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// bext TimeReference (sample count) as `hh:mm:ss.mmm` start timecode.
fn format_timecode(samples: u64, sample_rate: u32) -> String {
    if sample_rate == 0 {
        return "--".into();
    }
    let total_ms = (samples as f64 / sample_rate as f64 * 1000.0) as u64;
    let ms = total_ms % 1000;
    let s = (total_ms / 1000) % 60;
    let m = (total_ms / 60_000) % 60;
    let h = total_ms / 3_600_000;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(300);

fn relative_parent(root: Option<&Path>, file_path: &Path) -> String {
    let parent = match file_path.parent() {
        Some(p) => p,
        None => return String::new(),
    };
    if let Some(root) = root {
        if let Ok(rel) = parent.strip_prefix(root) {
            let s = rel.to_string_lossy();
            if s.is_empty() {
                return ".".into();
            }
            return s.into_owned();
        }
    }
    parent
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub struct BrowserPanel {
    prefs: PunksConfig,
    rebinding: Option<BrowserAction>,
    search_buf: String,
    last_typed_query: String,
    query_change_time: Instant,
    last_searched_query: String,
    volume: f32,
    /// Tracks the active tab between frames so the search box can be reloaded
    /// from the newly active tab's stored query when the user switches tabs.
    last_active_tab: usize,
    /// Last mouse-x we seeked to during a waveform drag, so a held-still cursor
    /// lets audio play forward instead of re-seeking every frame. `None` when
    /// not scrubbing.
    scrub_last_x: Option<f32>,
}

impl BrowserPanel {
    pub fn new() -> Self {
        let prefs = punks_core::config::load();
        let volume = prefs.volume;
        BrowserPanel {
            prefs,
            rebinding: None,
            search_buf: String::new(),
            last_typed_query: String::new(),
            query_change_time: Instant::now(),
            last_searched_query: String::new(),
            volume,
            last_active_tab: 0,
            scrub_last_x: None,
        }
    }

    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        on_drag_file: Option<&mut dyn FnMut(&Path)>,
    ) {
        browser.poll();

        // When the active tab changes, reload the search box from that tab's
        // stored query and resync the debounce trackers so we don't re-issue a
        // search for text the tab already has results for.
        if self.last_active_tab != browser.active_tab() {
            self.search_buf = browser.search_query().to_string();
            self.last_typed_query = self.search_buf.clone();
            self.last_searched_query = self.search_buf.clone();
            self.query_change_time = Instant::now();
            self.last_active_tab = browser.active_tab();
        }

        // Persist the deepest directory the user has navigated into, so the
        // browser restores exactly where they left off. One check here covers
        // every navigation path (keyboard, click, breadcrumb, Up, Browse) and
        // fires at most once per actual directory change. It only ever moves the
        // saved path forward — it never clobbers a saved location with `None`
        // when no folder is open (e.g. a temporarily unavailable drive).
        if let Some(dir) = browser.current_directory() {
            if self.prefs.last_directory.as_deref() != Some(dir) {
                self.prefs.last_directory = Some(dir.to_path_buf());
                punks_core::config::save(&self.prefs);
            }
        }

        // --- Tab bar: switch / drag-reorder / close / new ------------------
        let mut switch_to: Option<usize> = None;
        let mut close_idx: Option<usize> = None;
        let mut reorder: Option<(usize, usize)> = None;
        let mut open_new_tab = false;
        let tab_count = browser.tab_count();
        let active_tab = browser.active_tab();

        for i in 0..tab_count {
            if i > 0 {
                ui.same_line();
            }
            let (bg, bg_hover) = if i == active_tab {
                (TAB_ACTIVE_BG, TAB_ACTIVE_HOVER)
            } else {
                (TAB_INACTIVE_BG, TAB_INACTIVE_HOVER)
            };
            // The label and close glyph share one pushed background so they read
            // as a single tab rather than two detached buttons.
            let cbg = ui.push_style_color(imgui::StyleColor::Button, bg);
            let chov = ui.push_style_color(imgui::StyleColor::ButtonHovered, bg_hover);
            let cact = ui.push_style_color(imgui::StyleColor::ButtonActive, bg_hover);

            let title = browser.tab_title(i);
            if ui.button(format!("{title}##tab{i}")) {
                switch_to = Some(i);
            }

            // Drag this tab as a reorder source; payload is its index.
            if let Some(tooltip) = ui
                .drag_drop_source_config("TAB_REORDER")
                .flags(imgui::DragDropFlags::SOURCE_NO_PREVIEW_TOOLTIP)
                .begin_payload(i)
            {
                tooltip.end();
            }
            // Accept a dropped tab: the dragged tab lands at this index.
            if let Some(target) = ui.drag_drop_target() {
                if let Some(Ok(payload)) =
                    target.accept_payload::<usize, _>("TAB_REORDER", imgui::DragDropFlags::empty())
                {
                    reorder = Some((payload.data, i));
                }
                target.pop();
            }

            // Close glyph, attached flush to the label so the two read as one
            // tab. Hidden on the only tab so one always remains.
            if tab_count > 1 {
                ui.same_line_with_spacing(0.0, 0.0);
                let ctext = ui.push_style_color(imgui::StyleColor::Text, TAB_CLOSE_TEXT);
                if ui.button(format!("\u{00d7}##closetab{i}")) {
                    close_idx = Some(i);
                }
                ctext.pop();
            }

            cact.pop();
            chov.pop();
            cbg.pop();
        }
        ui.same_line();
        if ui.button("+##newtab") {
            open_new_tab = true;
        }
        ui.separator();

        // Apply deferred tab actions after the loop (mutating shifts indices).
        if let Some((from, to)) = reorder {
            browser.reorder_tab(from, to);
        }
        if let Some(i) = close_idx {
            browser.close_tab(i);
        }
        if open_new_tab {
            let start = browser.current_directory().map(|p| p.to_path_buf());
            browser.new_tab(start.as_deref());
        }
        if let Some(i) = switch_to {
            browser.switch_tab(i);
        }

        if ui.button("Browse...") {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(e) = browser.open_directory(&path) {
                    log::error!("failed to open directory: {e}");
                } else {
                    // last_directory is persisted centrally at the top of draw().
                    self.search_buf.clear();
                    self.last_typed_query.clear();
                    self.last_searched_query.clear();
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

        let avail = ui.content_region_avail();
        ui.set_next_item_width(avail[0]);
        ui.input_text("##search", &mut self.search_buf)
            .hint("Search...")
            .build();

        let search_focused = ui.is_item_active();

        if self.search_buf != self.last_typed_query {
            self.last_typed_query = self.search_buf.clone();
            self.query_change_time = Instant::now();
        }
        if self.query_change_time.elapsed() >= SEARCH_DEBOUNCE
            && self.last_typed_query != self.last_searched_query
        {
            self.last_searched_query = self.last_typed_query.clone();
            if self.last_searched_query.is_empty() {
                browser.clear_search();
            } else {
                browser.search(&self.last_searched_query);
            }
        }

        if browser.is_searching() && !browser.is_in_search_mode() {
            ui.text_disabled("Searching...");
        }

        let avail = ui.content_region_avail();
        // Reserve room below the list for: waveform + metadata line + transport.
        let list_height = (avail[1] - 132.0).max(100.0);
        let mut drag_requested: Option<PathBuf> = None;

        let up_key = parse_key(&self.prefs.keybinds.navigate_up).unwrap_or(Key::W);
        let down_key = parse_key(&self.prefs.keybinds.navigate_down).unwrap_or(Key::S);
        let back_key = parse_key(&self.prefs.keybinds.navigate_back).unwrap_or(Key::A);
        let conf_key = parse_key(&self.prefs.keybinds.confirm).unwrap_or(Key::D);

        let in_search = browser.is_in_search_mode();

        ui.child_window("file_list")
            .size([avail[0], list_height])
            .build(|| {
                if in_search {
                    self.draw_search_results(
                        ui,
                        browser,
                        &mut drag_requested,
                        search_focused,
                        up_key,
                        down_key,
                        back_key,
                    );
                } else {
                    self.draw_browse_list(
                        ui,
                        browser,
                        &mut drag_requested,
                        search_focused,
                        up_key,
                        down_key,
                        back_key,
                        conf_key,
                    );
                }
            });

        if let Some(path) = drag_requested.as_deref() {
            if let Some(on_drag_file) = on_drag_file {
                on_drag_file(path);
            }
            return;
        }

        ui.separator();

        // Panel-level keys (same focus gating as nav): Space toggles playback;
        // the tab keybinds switch / create / close tabs.
        if ui.is_window_focused() && !search_focused {
            if ui.is_key_pressed_no_repeat(Key::Space) {
                match browser.playback_status() {
                    PlaybackStatus::Playing { .. } | PlaybackStatus::Loading { .. } => {
                        browser.stop();
                    }
                    PlaybackStatus::Idle => {
                        if in_search {
                            if let Some(idx) = browser.search_selected() {
                                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                                    let path = e.path.clone();
                                    browser.play_file(&path);
                                }
                            }
                        } else {
                            browser.play_selected();
                        }
                    }
                }
            }

            let count = browser.tab_count();
            let active = browser.active_tab();
            let pressed =
                |bind: &str| parse_key(bind).is_some_and(|k| ui.is_key_pressed_no_repeat(k));
            if pressed(&self.prefs.keybinds.next_tab) {
                browser.switch_tab((active + 1) % count);
            } else if pressed(&self.prefs.keybinds.prev_tab) {
                browser.switch_tab((active + count - 1) % count);
            } else if pressed(&self.prefs.keybinds.new_tab) {
                let start = browser.current_directory().map(|p| p.to_path_buf());
                browser.new_tab(start.as_deref());
            } else if pressed(&self.prefs.keybinds.close_tab) {
                browser.close_tab(browser.active_tab());
            }
        }

        draw_waveform_widget(ui, browser, &mut self.scrub_last_x);

        // Container metadata (BWF bext) + long-file preview indicator, one line.
        // A blank line is reserved when absent so the layout doesn't jump.
        {
            let mut parts: Vec<String> = Vec::new();
            if let Some(info) = browser.current_track_info() {
                if let Some(desc) = info.metadata.description.as_deref() {
                    if !desc.is_empty() {
                        parts.push(desc.to_string());
                    }
                }
                if let Some(tc) = info.metadata.time_reference.filter(|&t| t > 0) {
                    parts.push(format!(
                        "TC {}",
                        format_timecode(tc, info.source_sample_rate)
                    ));
                }
                if info.truncated {
                    parts.push(format!(
                        "preview: first {} of {}",
                        format_hms(info.preview_duration.as_secs_f64()),
                        format_hms(info.source_duration.as_secs_f64()),
                    ));
                }
            }
            if parts.is_empty() {
                ui.new_line();
            } else {
                ui.text_disabled(parts.join("   \u{b7}   "));
            }
        }

        // Transport row: volume slider pinned to the right edge of the panel.
        let transport_x = ui.cursor_pos()[0];
        let transport_y = ui.cursor_pos()[1];
        let panel_width = ui.content_region_avail()[0];
        const VOLUME_SLIDER_WIDTH: f32 = 120.0;

        ui.set_cursor_pos([
            transport_x + (panel_width - VOLUME_SLIDER_WIDTH).max(0.0),
            transport_y,
        ]);
        ui.set_next_item_width(VOLUME_SLIDER_WIDTH);
        let mut vol = self.volume;
        let changed = ui
            .slider_config("##volume", 0.0_f32, 1.0_f32)
            .display_format("")
            .build(&mut vol);
        let hovered = ui.is_item_hovered();
        let committed = ui.is_item_deactivated_after_edit();
        if changed {
            self.volume = vol;
            browser.set_volume(self.volume);
        }
        if hovered {
            ui.tooltip_text(format!("Volume: {}%", (self.volume * 100.0).round() as i32));
        }
        if committed {
            self.prefs.volume = self.volume;
            punks_core::config::save(&self.prefs);
        }

        if let Some(err) = browser.last_error() {
            ui.text_colored([1.0, 0.3, 0.3, 1.0], err);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_search_results(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        drag_requested: &mut Option<PathBuf>,
        search_focused: bool,
        up_key: Key,
        down_key: Key,
        back_key: Key,
    ) {
        let count = match browser.search_results() {
            Some(r) if !r.is_empty() => r.len(),
            Some(_) => {
                ui.text_disabled("No results.");
                return;
            }
            None => {
                ui.text_disabled("Searching...");
                return;
            }
        };

        // Clone root so we don't hold a borrow on browser during keyboard
        // handling or the clipper loop.
        let root: Option<PathBuf> = browser.current_directory().map(|p| p.to_path_buf());

        // Keyboard navigation — mutable borrows happen here, before the clipper.
        if ui.is_window_focused() && !search_focused {
            if ui.is_key_pressed_no_repeat(up_key) {
                let idx = browser.search_selected().unwrap_or(0).saturating_sub(1);
                browser.select_search_result(idx);
                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                    let path = e.path.clone();
                    browser.play_file(&path);
                }
            }
            if ui.is_key_pressed_no_repeat(down_key) {
                let idx = (browser.search_selected().unwrap_or(0) + 1).min(count.saturating_sub(1));
                browser.select_search_result(idx);
                if let Some(e) = browser.search_results().and_then(|r| r.get(idx)) {
                    let path = e.path.clone();
                    browser.play_file(&path);
                }
            }
            if ui.is_key_pressed_no_repeat(back_key) {
                self.search_buf.clear();
                self.last_typed_query.clear();
                self.last_searched_query.clear();
                browser.clear_search();
                return;
            }
        }

        let selected = browser.search_selected();
        let mut click_action: Option<(usize, PathBuf)> = None;

        // Width-adaptive columns; the clipper iterates rows of `cols` items so
        // only visible rows allocate label strings.
        let avail_w = ui.content_region_avail()[0];
        let cols = column_count(avail_w);
        let col_w = avail_w / cols as f32;
        let num_rows = count.div_ceil(cols);

        let clip = imgui::ListClipper::new(num_rows as i32).begin(ui);
        'rows: for row in clip.iter() {
            for c in 0..cols {
                let i = row as usize * cols + c;
                if i >= count {
                    break;
                }
                if c > 0 {
                    ui.same_line_with_pos(c as f32 * col_w);
                }
                // Extract owned data in a short block so the borrow on browser
                // ends before we call any mutable method.
                let (label, path) = {
                    let results = browser.search_results().unwrap();
                    let e = &results[i];
                    let parent_hint = relative_parent(root.as_deref(), &e.path);
                    let label = format!("{}  ({})##sresult{}", e.name, parent_hint, i);
                    (label, e.path.clone())
                };

                let clicked = ui
                    .selectable_config(&label)
                    .selected(selected == Some(i))
                    .size([col_w - COLUMN_GUTTER, 0.0])
                    .build();
                if ui.is_item_hovered()
                    && ui.is_mouse_dragging_with_threshold(imgui::MouseButton::Left, -1.0)
                {
                    *drag_requested = Some(path);
                    break 'rows;
                }
                if clicked {
                    click_action = Some((i, path));
                }
            }
        }

        if let Some((i, path)) = click_action {
            browser.select_search_result(i);
            browser.play_file(&path);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_browse_list(
        &mut self,
        ui: &imgui::Ui,
        browser: &mut SampleBrowser,
        drag_requested: &mut Option<PathBuf>,
        search_focused: bool,
        up_key: Key,
        down_key: Key,
        back_key: Key,
        conf_key: Key,
    ) {
        let entry_count = browser.entries().len();

        if entry_count == 0 {
            if browser.current_directory().is_some() {
                ui.text_disabled("Empty directory.");
            } else {
                ui.text_disabled("No folder open. Click Browse to get started.");
            }
            return;
        }

        // Keyboard navigation — mutable borrows happen here, before the
        // clipper loop takes short immutable borrows to read entry data.
        let selected = browser.selected();
        if ui.is_window_focused() && !search_focused {
            if ui.is_key_pressed_no_repeat(up_key) {
                let idx = selected.unwrap_or(0).saturating_sub(1);
                browser.select(idx);
                browser.play_selected();
            }
            if ui.is_key_pressed_no_repeat(down_key) {
                let idx = (selected.unwrap_or(0) + 1).min(entry_count.saturating_sub(1));
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
                    let is_dir = browser.entries().get(i).map(|e| e.is_directory);
                    if is_dir == Some(true) {
                        if let Err(e) = browser.navigate_into(i) {
                            log::error!("navigate_into failed: {e}");
                        }
                    } else if is_dir == Some(false) {
                        browser.play_selected();
                    }
                }
            }
        }

        // Re-read selected in case keyboard nav changed it.
        let selected = browser.selected();
        let mut click_action: Option<(usize, bool, PathBuf)> = None;

        // Lay out entries in width-adaptive columns to use horizontal space.
        // The clipper iterates rows of `cols` items, so off-screen rows are
        // skipped and label strings are allocated only for visible items.
        let avail_w = ui.content_region_avail()[0];
        let cols = column_count(avail_w);
        let col_w = avail_w / cols as f32;
        let num_rows = entry_count.div_ceil(cols);

        let clip = imgui::ListClipper::new(num_rows as i32).begin(ui);
        'rows: for row in clip.iter() {
            for c in 0..cols {
                let i = row as usize * cols + c;
                if i >= entry_count {
                    break;
                }
                if c > 0 {
                    ui.same_line_with_pos(c as f32 * col_w);
                }

                // Extract owned data in a short block so the immutable borrow on
                // browser ends before we call any mutable method.
                let (label, is_dir, path) = {
                    let e = &browser.entries()[i];
                    let label = if e.is_directory {
                        format!("> {}##entry{}", e.name, i)
                    } else {
                        format!("{}##entry{}", e.name, i)
                    };
                    (label, e.is_directory, e.path.clone())
                };

                let is_selected = selected == Some(i);
                let size = [col_w - COLUMN_GUTTER, 0.0];
                let clicked = if is_dir {
                    let color = ui.push_style_color(imgui::StyleColor::Text, DIR_TEXT_COLOR);
                    let clicked = ui
                        .selectable_config(&label)
                        .selected(is_selected)
                        .size(size)
                        .build();
                    color.pop();
                    clicked
                } else {
                    ui.selectable_config(&label)
                        .selected(is_selected)
                        .size(size)
                        .build()
                };

                if !is_dir
                    && ui.is_item_hovered()
                    && ui.is_mouse_dragging_with_threshold(imgui::MouseButton::Left, -1.0)
                {
                    *drag_requested = Some(path);
                    break 'rows;
                }
                if clicked {
                    click_action = Some((i, is_dir, path));
                }
            }
        }

        // Apply click after the loop — avoids holding an immutable borrow
        // on browser.entries() while calling mutable browser methods.
        if let Some((i, is_dir, _)) = click_action {
            browser.select(i);
            if is_dir {
                if let Err(e) = browser.navigate_into(i) {
                    log::error!("navigate_into failed: {e}");
                }
            } else {
                browser.play_selected();
            }
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
                    // last_directory is persisted centrally at the top of draw().
                    if let Err(e) = browser.open_directory(&path) {
                        log::error!("failed to open directory: {e}");
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
// Subtle hover/scrub crosshair — dimmer than the opaque playhead.
const WAVEFORM_HOVER: [f32; 4] = [1.0, 1.0, 1.0, 0.35];

fn color_u32(c: [f32; 4]) -> u32 {
    let r = (c[0] * 255.0) as u32;
    let g = (c[1] * 255.0) as u32;
    let b = (c[2] * 255.0) as u32;
    let a = (c[3] * 255.0) as u32;
    (a << 24) | (b << 16) | (g << 8) | r
}

fn draw_waveform_widget(ui: &imgui::Ui, browser: &SampleBrowser, scrub_last_x: &mut Option<f32>) {
    let [cx, cy] = ui.cursor_screen_pos();
    let w = ui.content_region_avail()[0];
    const H: f32 = 64.0;

    // Interactive hit area (replaces the passive dummy) for hover + scrub.
    let clicked = ui.invisible_button("##waveform", [w, H]);
    let hovered = ui.is_item_hovered();
    let active = ui.is_item_active();
    let scrubbable = browser.loaded_duration().is_some();
    let mouse_x = ui.io().mouse_pos[0];

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

    // Hover / scrub crosshair + time label at the mouse position.
    if scrubbable && (hovered || active) {
        let mx = mouse_x.clamp(cx, cx + w);
        let hover = color_u32(WAVEFORM_HOVER);
        draw.add_line([mx, cy], [mx, cy + H], hover).build();
        let mid = cy + H / 2.0;
        draw.add_line([mx - 4.0, mid], [mx + 4.0, mid], hover)
            .build();
        if let Some(dur) = browser.loaded_duration() {
            let frac = ((mx - cx) / w).clamp(0.0, 1.0);
            let t = (dur.as_secs_f64() * frac as f64) as u64;
            let label = format!("{}:{:02}", t / 60, t % 60);
            let lx = (mx + 4.0).clamp(cx + 2.0, cx + w - 36.0);
            draw.add_text([lx, cy + 2.0], color_u32(WAVEFORM_TEXT), label);
        }
        ui.set_mouse_cursor(Some(imgui::MouseCursor::ResizeEW));
    }

    // Click seeks once; drag follows the cursor. Re-seek only when the cursor
    // moved >= 1px since the last seek, so a held-still cursor lets audio play
    // forward instead of re-triggering the same grain every frame.
    if scrubbable && (active || clicked) {
        let mx = mouse_x.clamp(cx, cx + w);
        if scrub_last_x.is_none_or(|lx| (mx - lx).abs() >= 1.0) {
            browser.seek_fraction((mx - cx) / w);
            *scrub_last_x = Some(mx);
        }
    } else {
        *scrub_last_x = None;
    }
}
