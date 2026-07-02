use std::collections::VecDeque;

pub enum OmniboxHit {
    Back,
    Forward,
    Refresh,
    Settings,
    Omnibox,
    None,
}

pub struct OmniboxState {
    pub is_focused: bool,
    pub input: String,
    pub cursor_position: usize,
    pub select_all_on_type: bool,
    pub history: VecDeque<String>,
    pub history_index: Option<usize>,
    pub scroll_offset: usize,

    pub text_version: u64,
    pub cursor_version: u64,
}

impl OmniboxState {
    fn mark_text_changed(&mut self) {
        self.text_version = self.text_version.wrapping_add(1);
        self.cursor_version = self.cursor_version.wrapping_add(1);
    }

    fn mark_cursor_changed(&mut self) {
        self.cursor_version = self.cursor_version.wrapping_add(1);
    }

    pub fn safe_cursor(&self) -> usize {
        let mut idx = self.cursor_position.min(self.input.len());
        while idx > 0 && !self.input.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    pub fn new() -> Self {
        Self {
            is_focused: false,
            input: String::new(),
            cursor_position: 0,
            select_all_on_type: false,
            history: VecDeque::with_capacity(16),
            history_index: None,
            scroll_offset: 0,
            
            text_version: 1,
            cursor_version: 1,
        }
    }

    pub fn focus(&mut self, current_url: &str) {
        self.is_focused = true;
        self.input = current_url.to_string();
        self.cursor_position = self.input.len();
        self.select_all_on_type = true;
        self.history_index = None;
        self.scroll_offset = 0;
        self.mark_text_changed();
    }

    pub fn defocus(&mut self) {
        self.is_focused = false;
        self.select_all_on_type = false;
        self.history_index = None;
        self.scroll_offset = 0;
        self.mark_text_changed();
    }

    pub fn insert_char(&mut self, c: char) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
        }
        if self.cursor_position <= self.input.len() {
            self.input.insert(self.cursor_position, c);
            self.cursor_position += c.len_utf8();
            self.mark_text_changed();
        }
    }

    pub fn insert_str(&mut self, text: &str) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
        }
        if self.cursor_position <= self.input.len() {
            self.input.insert_str(self.cursor_position, text);
            self.cursor_position += text.len();
            self.mark_text_changed();
        }
    }

    pub fn backspace(&mut self) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
            self.mark_text_changed();
            return;
        }
        let safe_cur = self.safe_cursor();
        if safe_cur > 0 {
            let prev_char_idx = self.input[..safe_cur]
                .chars()
                .last()
                .map(|c| safe_cur - c.len_utf8());
            if let Some(idx) = prev_char_idx {
                self.input.remove(idx);
                self.cursor_position = idx;
                self.mark_text_changed();
            }
        }
    }

    pub fn delete(&mut self) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
            self.mark_text_changed();
            return;
        }
        let safe_cur = self.safe_cursor();
        if safe_cur < self.input.len() {
            self.input.remove(safe_cur);
            self.mark_text_changed();
        }
    }

    pub fn home(&mut self) {
        self.select_all_on_type = false;
        self.cursor_position = 0;
        self.mark_cursor_changed();
    }

    pub fn end(&mut self) {
        self.select_all_on_type = false;
        self.cursor_position = self.input.len();
        self.mark_cursor_changed();
    }

    pub fn arrow_left(&mut self) {
        self.select_all_on_type = false;
        let safe_cur = self.safe_cursor();
        if safe_cur > 0 {
            let prev_char_idx = self.input[..safe_cur]
                .chars()
                .last()
                .map(|c| safe_cur - c.len_utf8());
            if let Some(idx) = prev_char_idx {
                self.cursor_position = idx;
                self.mark_cursor_changed();
            }
        }
    }

    pub fn arrow_right(&mut self) {
        self.select_all_on_type = false;
        let safe_cur = self.safe_cursor();
        if safe_cur < self.input.len() {
            if let Some(c) = self.input[safe_cur..].chars().next() {
                self.cursor_position = safe_cur + c.len_utf8();
                self.mark_cursor_changed();
            }
        }
    }

    pub fn arrow_up(&mut self) {
        self.select_all_on_type = false;
        if self.history.is_empty() { return; }

        let new_idx = match self.history_index {
            Some(idx) => if idx + 1 < self.history.len() { idx + 1 } else { idx },
            None => 0,
        };
        self.history_index = Some(new_idx);
        self.input = self.history[new_idx].clone();
        self.cursor_position = self.input.len();
        self.mark_text_changed();
    }

    pub fn arrow_down(&mut self) {
        self.select_all_on_type = false;
        if let Some(idx) = self.history_index {
            if idx > 0 {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.input = self.history[new_idx].clone();
                self.cursor_position = self.input.len();
                self.mark_text_changed();
            } else {
                self.history_index = None;
                self.input.clear();
                self.cursor_position = 0;
                self.mark_text_changed();
            }
        }
    }

    pub fn push_history(&mut self, url: String) {
        if self.history.front() != Some(&url) {
            self.history.push_front(url);
            if self.history.len() > 16 {
                self.history.pop_back();
            }
        }
    }

    pub fn update_scroll(&mut self, visible_chars: usize) -> bool {
        if !self.is_focused {
            let changed = self.scroll_offset != 0;
            self.scroll_offset = 0;
            return changed;
        }
        if visible_chars == 0 {
            let changed = self.scroll_offset != 0;
            self.scroll_offset = 0;
            return changed;
        }
        let safe_cur = self.safe_cursor();
        let cursor_char_idx = self.input[..safe_cur].chars().count();
        let old_offset = self.scroll_offset;
        
        if cursor_char_idx < self.scroll_offset {
            self.scroll_offset = cursor_char_idx;
        } else if cursor_char_idx >= self.scroll_offset + visible_chars {
            self.scroll_offset = cursor_char_idx - visible_chars + 1;
        }
        old_offset != self.scroll_offset
    }
}

pub struct OmniboxLayout {
    pub last_text_version: u64,
    pub last_cursor_version: u64,
    pub last_url: String,
    pub last_width: usize,
    
    pub cached_visible_text: String,
    pub cached_selection_width: usize,
    pub cached_cursor_x_offset: Option<usize>,

    pub cursor_blink_visible: bool,
    pub last_cursor_blink: std::time::Instant,
}

impl OmniboxLayout {
    pub fn new() -> Self {
        Self {
            last_text_version: 0,
            last_cursor_version: 0,
            last_url: String::new(),
            last_width: 0,
            cached_visible_text: String::new(),
            cached_selection_width: 0,
            cached_cursor_x_offset: None,
            cursor_blink_visible: true,
            last_cursor_blink: std::time::Instant::now(),
        }
    }

    pub fn update(&mut self, state: &mut OmniboxState, window_width: usize, current_url: &str) {
        let (_, omnibox_w) = get_omnibox_rect(window_width);
        if omnibox_w <= 10 { return; }

        let width_changed = self.last_width != omnibox_w;
        let url_changed = !state.is_focused && self.last_url != current_url;
        let mut force_text_rebuild = width_changed || url_changed;

        self.last_width = omnibox_w;
        if !state.is_focused {
            self.last_url = current_url.to_string();
        }

        let visible_chars = omnibox_w.saturating_sub(20) / 8;
        if state.update_scroll(visible_chars) {
            force_text_rebuild = true;
        }

        let text_changed = force_text_rebuild || self.last_text_version != state.text_version;
        let cursor_changed = text_changed || self.last_cursor_version != state.cursor_version;

        if text_changed || cursor_changed {
            self.cursor_blink_visible = true;
            self.last_cursor_blink = std::time::Instant::now();
        }

        if text_changed {
            let display_text = if state.is_focused { &state.input } else { current_url };
            let chars: Vec<char> = display_text.chars().collect();
            let start_idx = if state.is_focused { state.scroll_offset.min(chars.len()) } else { 0 };
            
            self.cached_visible_text = chars[start_idx..].iter().collect();

            if state.is_focused && state.select_all_on_type && !display_text.is_empty() {
                self.cached_selection_width = (self.cached_visible_text.chars().count() * 8).min(omnibox_w.saturating_sub(20));
            } else {
                self.cached_selection_width = 0;
            }
            
            self.last_text_version = state.text_version;
        }

        if cursor_changed {
            if state.is_focused && !state.select_all_on_type {
                let safe_cur = state.safe_cursor();
                let chars_before_cursor = state.input[..safe_cur].chars().count();
                if chars_before_cursor >= state.scroll_offset {
                    let offset = (chars_before_cursor - state.scroll_offset) * 8;
                    if offset < omnibox_w.saturating_sub(10) {
                        self.cached_cursor_x_offset = Some(offset);
                    } else {
                        self.cached_cursor_x_offset = None;
                    }
                } else {
                    self.cached_cursor_x_offset = None;
                }
            } else {
                self.cached_cursor_x_offset = None;
            }
            
            self.last_cursor_version = state.cursor_version;
        }
    }
}

fn minimal_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn resolve_navigation_target(input: &str, search_engine: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() { return String::new(); }

    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("file://")
        || trimmed.starts_with("petal://")
    {
        return trimmed.to_string();
    }

    if trimmed.starts_with("localhost:") || trimmed.starts_with("127.0.0.1") {
        return format!("http://{}", trimmed);
    }

    let looks_like_domain = trimmed.contains('.') && !trimmed.contains(' ');
    if looks_like_domain {
        return format!("https://{}", trimmed);
    }

    search_engine.replace("{}", &minimal_encode(trimmed))
}

pub fn hit_test_omnibox(cursor_x: f64, width: usize) -> OmniboxHit {
    let mut current_x = 10;
    let button_w = 36;
    let x = cursor_x as usize;

    // Back
    if width > 80 {
        if x >= current_x && x < current_x + button_w { return OmniboxHit::Back; }
        current_x += button_w + 6;
    }
    // Forward
    if width > 120 {
        if x >= current_x && x < current_x + button_w { return OmniboxHit::Forward; }
        current_x += button_w + 6;
    }
    // Refresh
    if width > 180 {
        if x >= current_x && x < current_x + button_w { return OmniboxHit::Refresh; }
        current_x += button_w + 6;
    }

    // Settings
    let mut settings_w = 0;
    if width > 220 {
        settings_w = button_w + 10;
        let settings_x = width.saturating_sub(button_w + 10);
        if x >= settings_x && x < settings_x + button_w { return OmniboxHit::Settings; }
    }

    let omnibox_x = current_x + 4;
    let omnibox_w = width.saturating_sub(omnibox_x + settings_w).saturating_sub(4);
    
    if omnibox_w > 10 && x >= omnibox_x && x < omnibox_x + omnibox_w {
        return OmniboxHit::Omnibox;
    }

    OmniboxHit::None
}

pub fn get_omnibox_rect(width: usize) -> (usize, usize) {
    let button_w = 36;
    let mut current_x = 10;
    if width > 80 { current_x += button_w + 6; }
    if width > 120 { current_x += button_w + 6; }
    if width > 180 { current_x += button_w + 6; }

    let mut settings_w = 0;
    if width > 220 {
        settings_w = button_w + 10;
    }

    let omnibox_x = current_x + 4;
    let omnibox_w = width.saturating_sub(omnibox_x + settings_w).saturating_sub(4);
    
    (omnibox_x, omnibox_w)
}

pub fn render_omnibox_static(buffer: &mut [u32], width: usize, is_focused: bool) {
    if width < 2 { return; }

    let bg_color = 0xFF_28_28_28; // Matches active tab background
    crate::ui::clear_rect(buffer, width, 0, 0, width, crate::ui::OMNIBOX_HEIGHT as usize, bg_color);

    let nav_y    = 8; // Offset relativo ao início do buffer (já que a camada omnibox começa no y=0 dela própria)
    let button_h = 30;
    let button_w = 36;
    let btn_bg     = 0xFF_3C_3C_3C;
    let icon_color = 0xFF_DD_DD_DD;

    let mut current_x = 10;

    // Back `<`
    if width > 80 {
        crate::ui::draw_beveled_rect(buffer, width, current_x, nav_y, button_w, button_h, btn_bg);
        crate::ui::draw_char(buffer, width, current_x + 14, nav_y + 7, '<', icon_color);
        current_x += button_w + 6;
    }

    // Forward `>`
    if width > 120 {
        crate::ui::draw_beveled_rect(buffer, width, current_x, nav_y, button_w, button_h, btn_bg);
        crate::ui::draw_char(buffer, width, current_x + 14, nav_y + 7, '>', icon_color);
        current_x += button_w + 6;
    }

    // Refresh `C`
    if width > 180 {
        crate::ui::draw_beveled_rect(buffer, width, current_x, nav_y, button_w, button_h, btn_bg);
        crate::ui::draw_char(buffer, width, current_x + 14, nav_y + 7, 'C', icon_color);
        current_x += button_w + 6;
    }

    // Settings `S`
    let mut settings_w = 0;
    if width > 220 {
        settings_w = button_w + 10;
        let settings_x = width.saturating_sub(button_w + 10);
        crate::ui::draw_beveled_rect(buffer, width, settings_x, nav_y, button_w, button_h, btn_bg);
        crate::ui::draw_char(buffer, width, settings_x + 14, nav_y + 7, 'S', icon_color);
    }

    // Omnibox field background
    let omnibox_x = current_x + 4;
    let omnibox_w = width.saturating_sub(omnibox_x + settings_w).saturating_sub(4);
    
    if omnibox_w > 10 {
        let field_bg = if is_focused { 0xFF_00_00_00 } else { 0xFF_11_11_11 };
        crate::ui::draw_beveled_rect(buffer, width, omnibox_x, nav_y, omnibox_w, button_h, field_bg);
    }
}

pub fn render_omnibox_dynamic(buffer: &mut [u32], window_width: usize, layout: &OmniboxLayout) {
    if window_width < 2 { return; }
    
    let (omnibox_x, omnibox_w) = get_omnibox_rect(window_width);
    if omnibox_w <= 10 { return; }

    let nav_y = crate::ui::TABBAR_HEIGHT as usize + 8;

    if layout.cached_selection_width > 0 {
        crate::ui::clear_rect(buffer, window_width, omnibox_x + 10, nav_y + 7, layout.cached_selection_width, 16, 0xFF_00_55_AA);
    }

    crate::ui::draw_string(
        buffer, window_width,
        omnibox_x + 10, nav_y + 7,
        &layout.cached_visible_text,
        0xFF_E0_E0_E0,
        omnibox_w.saturating_sub(20),
    );

    if layout.cursor_blink_visible {
        if let Some(cursor_offset) = layout.cached_cursor_x_offset {
            crate::ui::clear_rect(buffer, window_width, omnibox_x + 10 + cursor_offset, nav_y + 7, 2, 16, 0xFF_FF_FF_FF);
        }
    }
}