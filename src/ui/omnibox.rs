use std::collections::VecDeque;

pub struct OmniboxState {
    pub is_focused: bool,
    pub input: String,
    pub cursor_position: usize,
    pub select_all_on_type: bool,
    pub history: VecDeque<String>,
    pub history_index: Option<usize>,
    pub scroll_offset: usize,
}

impl OmniboxState {
    pub fn new() -> Self {
        Self {
            is_focused: false,
            input: String::new(),
            cursor_position: 0,
            select_all_on_type: false,
            history: VecDeque::with_capacity(16),
            history_index: None,
            scroll_offset: 0,
        }
    }

    pub fn focus(&mut self, current_url: &str) {
        self.is_focused = true;
        self.input = current_url.to_string();
        self.cursor_position = self.input.len();
        self.select_all_on_type = true;
        self.history_index = None;
        self.scroll_offset = 0;
    }

    pub fn defocus(&mut self) {
        self.is_focused = false;
        self.select_all_on_type = false;
        self.history_index = None;
        self.scroll_offset = 0;
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
        }
    }

    pub fn backspace(&mut self) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
            return;
        }
        if self.cursor_position > 0 {
            // Remove the last char before cursor by checking char boundaries
            let prev_char_idx = self.input[..self.cursor_position].chars().last().map(|c| self.cursor_position - c.len_utf8());
            if let Some(idx) = prev_char_idx {
                self.input.remove(idx);
                self.cursor_position = idx;
            }
        }
    }

    pub fn delete(&mut self) {
        if self.select_all_on_type {
            self.input.clear();
            self.cursor_position = 0;
            self.select_all_on_type = false;
            return;
        }
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    pub fn home(&mut self) {
        self.select_all_on_type = false;
        self.cursor_position = 0;
    }

    pub fn end(&mut self) {
        self.select_all_on_type = false;
        self.cursor_position = self.input.len();
    }

    pub fn arrow_left(&mut self) {
        self.select_all_on_type = false;
        if self.cursor_position > 0 {
            let prev_char_idx = self.input[..self.cursor_position].chars().last().map(|c| self.cursor_position - c.len_utf8());
            if let Some(idx) = prev_char_idx {
                self.cursor_position = idx;
            }
        }
    }

    pub fn arrow_right(&mut self) {
        self.select_all_on_type = false;
        if self.cursor_position < self.input.len() {
            let next_char_len = self.input[self.cursor_position..].chars().next().unwrap().len_utf8();
            self.cursor_position += next_char_len;
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
    }

    pub fn arrow_down(&mut self) {
        self.select_all_on_type = false;
        if let Some(idx) = self.history_index {
            if idx > 0 {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.input = self.history[new_idx].clone();
                self.cursor_position = self.input.len();
            } else {
                self.history_index = None;
                self.input.clear();
                self.cursor_position = 0;
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

    pub fn update_scroll(&mut self, visible_chars: usize) {
        if !self.is_focused {
            self.scroll_offset = 0;
            return;
        }
        let cursor_char_idx = self.input[..self.cursor_position].chars().count();
        if cursor_char_idx < self.scroll_offset {
            self.scroll_offset = cursor_char_idx;
        } else if cursor_char_idx >= self.scroll_offset + visible_chars {
            self.scroll_offset = cursor_char_idx - visible_chars + 1;
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

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("file://") || trimmed.starts_with("magma://") {
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

pub fn render_omnibox(buffer: &mut [u32], width: usize, state: &mut OmniboxState, current_url: &str) {
    let bg_color = 0xFF_28_28_28; // Matches active tab background
    crate::ui::clear_rect(buffer, width, 0, crate::ui::TABBAR_HEIGHT as usize, width, crate::ui::OMNIBOX_HEIGHT as usize, bg_color);

    let nav_y = crate::ui::TABBAR_HEIGHT as usize + 8;
    let button_h = 30;
    let button_w = 36;
    let btn_bg = 0xFF_3C_3C_3C;
    let icon_color = 0xFF_DD_DD_DD;

    // Back `<`
    crate::ui::draw_beveled_rect(buffer, width, 10, nav_y, button_w, button_h, btn_bg);
    crate::ui::draw_char(buffer, width, 10 + 14, nav_y + 7, '<', icon_color);

    // Forward `>`
    crate::ui::draw_beveled_rect(buffer, width, 52, nav_y, button_w, button_h, btn_bg);
    crate::ui::draw_char(buffer, width, 52 + 14, nav_y + 7, '>', icon_color);

    // Refresh `C`
    crate::ui::draw_beveled_rect(buffer, width, 94, nav_y, button_w, button_h, btn_bg);
    crate::ui::draw_char(buffer, width, 94 + 14, nav_y + 7, 'C', icon_color);

    // Settings `S`
    let settings_x = width.saturating_sub(46);
    crate::ui::draw_beveled_rect(buffer, width, settings_x, nav_y, button_w, button_h, btn_bg);
    crate::ui::draw_char(buffer, width, settings_x + 14, nav_y + 7, 'S', icon_color);

    // Omnibox field
    let omnibox_x = 140;
    let omnibox_w = width.saturating_sub(140 + 56); // Room for settings
    
    let field_bg = if state.is_focused { 0xFF_00_00_00 } else { 0xFF_11_11_11 };
    crate::ui::draw_beveled_rect(buffer, width, omnibox_x, nav_y, omnibox_w, button_h, field_bg);

    let visible_chars = omnibox_w.saturating_sub(20) / 8;
    state.update_scroll(visible_chars);

    let display_text = if state.is_focused { &state.input } else { current_url };
    let chars: Vec<char> = display_text.chars().collect();
    let start_idx = if state.is_focused { state.scroll_offset.min(chars.len()) } else { 0 };
    let visible_text: String = chars[start_idx..].iter().collect();

    if state.is_focused && state.select_all_on_type && !display_text.is_empty() {
        let sel_w = (visible_text.chars().count() * 8).min(omnibox_w - 20);
        crate::ui::clear_rect(buffer, width, omnibox_x + 10, nav_y + 7, sel_w, 16, 0xFF_00_55_AA);
    }

    crate::ui::draw_string(buffer, width, omnibox_x + 10, nav_y + 7, &visible_text, 0xFF_E0_E0_E0, omnibox_w.saturating_sub(20));

    if state.is_focused && !state.select_all_on_type {
        let chars_before_cursor = state.input[..state.cursor_position].chars().count();
        if chars_before_cursor >= state.scroll_offset {
            let cursor_x = omnibox_x + 10 + ((chars_before_cursor - state.scroll_offset) * 8);
            if cursor_x < omnibox_x + omnibox_w - 10 {
                crate::ui::clear_rect(buffer, width, cursor_x, nav_y + 7, 2, 16, 0xFF_FF_FF_FF);
            }
        }
    }
}
