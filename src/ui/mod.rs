pub mod font;
pub mod omnibox;
pub mod settings;

pub const TABBAR_HEIGHT: u32 = 40;
pub const OMNIBOX_HEIGHT: u32 = 46;
pub const CHROME_HEIGHT: u32 = TABBAR_HEIGHT + OMNIBOX_HEIGHT;

#[derive(Debug, Clone, Copy, Default)]
pub struct DirtyRegion {
    pub omnibox: bool,
    pub tabbar: bool,
    pub whole_window: bool,
}

impl DirtyRegion {
    pub fn new() -> Self { Self::default() }
    
    pub fn invalidate_omnibox(&mut self) { self.omnibox = true; }
    pub fn invalidate_tabbar(&mut self) { self.tabbar = true; }
    pub fn invalidate_chrome(&mut self) { self.omnibox = true; self.tabbar = true; }
    pub fn invalidate_all(&mut self) { self.whole_window = true; self.omnibox = true; self.tabbar = true; }
    
    pub fn needs_redraw(&self) -> bool { self.omnibox || self.tabbar || self.whole_window }
    
    pub fn reset(&mut self) { *self = Self::default(); }
}

pub struct LayerCache {
    pub buffer: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub valid: bool,
}

impl LayerCache {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            width: 0,
            height: 0,
            valid: false,
        }
    }

    pub fn ensure_size(&mut self, width: usize, height: usize) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.buffer.resize(width * height, 0);
            self.valid = false;
        }
    }

    pub fn invalidate(&mut self) { self.valid = false; }
}

pub struct UICompositor {
    pub tabbar: LayerCache,
    pub static_omnibox: LayerCache,
    pub omnibox_is_focused: bool,
}

impl UICompositor {
    pub fn new() -> Self {
        Self {
            tabbar: LayerCache::new(),
            static_omnibox: LayerCache::new(),
            omnibox_is_focused: false,
        }
    }
}

use crate::fsm::tab_manager::Tab;

pub const TAB_WIDTH: usize = 220;
pub const TAB_MARGIN_LEFT: usize = 8;

pub enum TabHit {
    Tab(usize),
    CloseButton(usize),
    NewTabButton,
    None,
}

pub fn hit_test_tab_bar(cursor_x: f64, num_tabs: usize, window_width: f64) -> TabHit {
    let x = cursor_x as usize;

    for i in 0..num_tabs {
        let start_x = i * TAB_WIDTH + TAB_MARGIN_LEFT;
        if start_x >= window_width as usize { break; }

        let w = if start_x + TAB_WIDTH > window_width as usize {
            window_width as usize - start_x // seguro: start_x < window_width garantido pelo break acima
        } else {
            TAB_WIDTH
        };
        let end_x = start_x + w;

        if x >= start_x && x < end_x {
            // FIX: saturating_sub previne underflow quando a aba é mais estreita que 24px.
            let close_x = (start_x + w).saturating_sub(24);
            // FIX: espelha a condição de render_tab_bar (botão X só é desenhado quando w > 44,
            // ou seja, close_x > start_x + 20). Também protege o caso de saturação em que
            // close_x == 0: 0 > start_x + 20 (com start_x >= TAB_MARGIN_LEFT = 8) é sempre falso.
            if close_x > start_x + 20 && x >= close_x {
                return TabHit::CloseButton(i);
            } else {
                return TabHit::Tab(i);
            }
        }
    }

    let mut plus_x = num_tabs * TAB_WIDTH + 16;
    let w_usize = window_width as usize;
    if plus_x + 16 > w_usize {
        plus_x = w_usize.saturating_sub(20);
    }
    if plus_x < w_usize {
        // Roughly 16px wide hitbox for the + button
        if x >= plus_x.saturating_sub(4) && x <= plus_x + 16 {
            return TabHit::NewTabButton;
        }
    }

    TabHit::None
}

/// Limpa o buffer com uma cor de fundo
pub fn clear_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    if buf_width == 0 { return; }
    let buf_height = buffer.len() / buf_width;

    let start_y = y.min(buf_height);
    let end_y = y.saturating_add(h).min(buf_height);
    
    let start_x = x.min(buf_width);
    let end_x = x.saturating_add(w).min(buf_width);

    for row in start_y..end_y {
        let row_base = row * buf_width;
        for col in start_x..end_x {
            buffer[row_base + col] = color;
        }
    }
}

/// Desenha um retângulo com cantos removidos (beveled)
pub fn draw_beveled_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    if w < 2 || h < 2 {
        return clear_rect(buffer, buf_width, x, y, w, h, color);
    }
    if buf_width == 0 { return; }
    let buf_height = buffer.len() / buf_width;

    let start_y = y.min(buf_height);
    let end_y = y.saturating_add(h).min(buf_height);

    for py in start_y..end_y {
        let row_in_rect = py - y;

        let mut start_col = x;
        let mut end_col = x.saturating_add(w);

        if row_in_rect == 0 || row_in_rect == h - 1 {
            start_col = start_col.saturating_add(1);
            end_col = end_col.saturating_sub(1);
        }

        let start_x = start_col.min(buf_width);
        let end_x = end_col.min(buf_width);

        let row_base = py * buf_width;
        for px in start_x..end_x {
            buffer[row_base + px] = color;
        }
    }
}

/// Desenha um caractere 8x16 usando a fonte bitmap com clipping explícito opcional.
pub fn draw_char_clipped(
    buffer: &mut [u32], buf_width: usize,
    x: usize, y: usize, c: char, color: u32,
    clip_x: usize, clip_y: usize, clip_w: usize, clip_h: usize
) {
    if buf_width == 0 { return; }
    let buf_height = buffer.len() / buf_width;

    let code = c as usize;
    let glyph = font::FONT_8X16.get(code).unwrap_or(&font::FONT_8X16[63]);

    let min_y = clip_y.min(buf_height).max(y);
    let max_y = clip_y.saturating_add(clip_h).min(buf_height).min(y.saturating_add(16));
    if min_y >= max_y { return; }

    let min_x = clip_x.min(buf_width).max(x);
    let max_x = clip_x.saturating_add(clip_w).min(buf_width).min(x.saturating_add(8));
    if min_x >= max_x { return; }

    // Fast-path para o caso em que o caractere está totalmente visível (sem clipping parcial)
    if min_x == x && max_x == x + 8 && min_y == y && max_y == y + 16 {
        for (row_idx, &row_val) in glyph.iter().enumerate() {
            if row_val == 0 { continue; }
            let row_base = (y + row_idx) * buf_width + x;
            if (row_val & 0x80) != 0 { buffer[row_base]     = color; }
            if (row_val & 0x40) != 0 { buffer[row_base + 1] = color; }
            if (row_val & 0x20) != 0 { buffer[row_base + 2] = color; }
            if (row_val & 0x10) != 0 { buffer[row_base + 3] = color; }
            if (row_val & 0x08) != 0 { buffer[row_base + 4] = color; }
            if (row_val & 0x04) != 0 { buffer[row_base + 5] = color; }
            if (row_val & 0x02) != 0 { buffer[row_base + 6] = color; }
            if (row_val & 0x01) != 0 { buffer[row_base + 7] = color; }
        }
        return;
    }

    // Caminho lento para caracteres clipados
    for py in min_y..max_y {
        let row_idx = py - y;
        let row_val = glyph[row_idx];
        if row_val == 0 { continue; }
        
        let row_base = py * buf_width;
        for px in min_x..max_x {
            let col_idx = px - x;
            if (row_val & (1 << (7 - col_idx))) != 0 {
                buffer[row_base + px] = color;
            }
        }
    }
}

/// Desenha um caractere 8x16 usando a fonte bitmap (com clipping baseado apenas nos limites do buffer)
pub fn draw_char(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, c: char, color: u32) {
    let buf_height = if buf_width > 0 { buffer.len() / buf_width } else { 0 };
    draw_char_clipped(buffer, buf_width, x, y, c, color, 0, 0, buf_width, buf_height);
}

/// Desenha uma string com a fonte 8x16, cortando píxels parcialmente (clipping suave)
pub fn draw_string(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, text: &str, color: u32, max_width: usize) {
    let buf_height = if buf_width > 0 { buffer.len() / buf_width } else { 0 };
    let clip_w = max_width.min(buf_width.saturating_sub(x));
    
    let mut current_x = x;
    let char_width = 8;
    for c in text.chars() {
        if current_x >= x.saturating_add(max_width) {
            break; // Já extrapolamos a caixa totalmente
        }
        draw_char_clipped(buffer, buf_width, current_x, y, c, color, x, y, clip_w, buf_height.saturating_sub(y));
        current_x = current_x.saturating_add(char_width);
    }
}

/// Renderiza a barra de abas completa no buffer
pub fn render_tab_bar(buffer: &mut [u32], width: usize, tabs: &[Tab], active_index: usize) {
    if width < 2 { return; }

    let bg_color     = 0xFF_11_11_11; // Darkest background (Title bar)
    let fg_color     = 0xFF_D4_D4_D4;
    let active_bg    = 0xFF_28_28_28; // Matches Omnibox background
    let inactive_bg  = 0xFF_1C_1C_1C;

    // Title bar background
    clear_rect(buffer, width, 0, 0, width, TABBAR_HEIGHT as usize, bg_color);

    for (i, tab) in tabs.iter().enumerate() {
        let start_x = i * TAB_WIDTH + TAB_MARGIN_LEFT; // Margin from left
        if start_x >= width { break; }

        let is_active = i == active_index;
        let t_bg = if is_active { active_bg } else { inactive_bg };

        let w = if start_x + TAB_WIDTH > width { width - start_x } else { TAB_WIDTH };
        // seguro: start_x < width garantido pelo break acima, logo width - start_x >= 1

        // Tab shape: top rounded, bottom flat.
        let tab_y = 8; // Tabs are padded from the top
        // FIX: saturating_sub defensivo — atualmente TABBAR_HEIGHT(40) - tab_y(8) = 32.
        // Protege caso TABBAR_HEIGHT seja reduzida no futuro abaixo de tab_y.
        let tab_h = (TABBAR_HEIGHT as usize).saturating_sub(tab_y);

        // Draw the main tab block (beveled top corners)
        // FIX: saturating_sub evita underflow de usize quando w é muito estreito
        clear_rect(buffer, width, start_x + 2, tab_y, w.saturating_sub(4), tab_h, t_bg); // Body
        clear_rect(buffer, width, start_x + 1, tab_y + 1, w.saturating_sub(2), tab_h.saturating_sub(1), t_bg); // Mid bevel
        clear_rect(buffer, width, start_x,     tab_y + 2, w,                   tab_h.saturating_sub(2), t_bg); // Full width base

        // Ícone minúsculo / Margem e Título
        let text_x = start_x + 16;
        // FIX: saturating_sub evita underflow caso tab_h fique menor que 16
        let text_y = tab_y + tab_h.saturating_sub(16) / 2; // Centralizado no bloco da aba

        let title_to_draw = if tab.title.is_empty() { "Nova Aba" } else { &tab.title };
        let active_fg = if is_active { 0xFF_FF_FF_FF } else { 0xFF_99_99_99 };

        draw_string(buffer, width, text_x, text_y, title_to_draw, active_fg, w.saturating_sub(40));

        // Botão X
        // FIX: saturating_sub evita underflow quando (start_x + w) < 24
        let close_x = (start_x + w).saturating_sub(24);
        // A guarda `close_x > start_x + 20` permanece inalterada e continua correta após a fix:
        //   - saturação (close_x == 0): 0 > start_x + 20 é sempre falso → botão não desenhado ✓
        //   - caso normal (sem saturação): equivale a w > 44, mesma lógica de antes ✓
        if close_x > start_x + 20 {
            draw_char(buffer, width, close_x, text_y, 'x', 0xFF_77_77_77);
        }
    }

    // Botão nova aba '+'
    let mut plus_x = tabs.len() * TAB_WIDTH + 16;
    if plus_x + 16 > width {
        plus_x = width.saturating_sub(20);
    }
    if plus_x < width {
        let plus_y = 12;
        draw_char(buffer, width, plus_x, plus_y, '+', fg_color);
    }
}