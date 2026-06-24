pub mod font;
pub mod omnibox;
pub mod settings;

pub const TABBAR_HEIGHT: u32 = 40;
pub const OMNIBOX_HEIGHT: u32 = 46;
pub const CHROME_HEIGHT: u32 = TABBAR_HEIGHT + OMNIBOX_HEIGHT;

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
        
        let w = if start_x + TAB_WIDTH > window_width as usize { window_width as usize - start_x } else { TAB_WIDTH };
        let end_x = start_x + w;
        
        if x >= start_x && x < end_x {
            let close_x = start_x + w - 24;
            // The close button 'x' is at close_x
            if x >= close_x && x < start_x + w {
                return TabHit::CloseButton(i);
            } else {
                return TabHit::Tab(i);
            }
        }
    }
    
    let plus_x = num_tabs * TAB_WIDTH + 16;
    if plus_x < window_width as usize {
        // Roughly 16px wide hitbox for the + button
        if x >= plus_x.saturating_sub(4) && x <= plus_x + 16 {
            return TabHit::NewTabButton;
        }
    }
    
    TabHit::None
}

/// Limpa o buffer com uma cor de fundo
pub fn clear_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for row in y..(y + h) {
        let offset = row * buf_width + x;
        for col in 0..w {
            if offset + col < buffer.len() {
                buffer[offset + col] = color;
            }
        }
    }
}

/// Desenha um retângulo com cantos removidos (beveled)
pub fn draw_beveled_rect(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    if w < 2 || h < 2 {
        return clear_rect(buffer, buf_width, x, y, w, h, color);
    }
    for row in 0..h {
        let py = y + row;
        let mut start_col = 0;
        let mut end_col = w;
        
        if row == 0 || row == h - 1 {
            start_col = 1;
            end_col = w - 1;
        }
        
        for col in start_col..end_col {
            let px = x + col;
            let offset = py * buf_width + px;
            if offset < buffer.len() {
                buffer[offset] = color;
            }
        }
    }
}

/// Desenha um caractere 8x16 usando a fonte bitmap
pub fn draw_char(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, c: char, color: u32) {
    let mut code = c as usize;
    if code >= 128 {
        code = '?' as usize;
    }
    let glyph = &font::FONT_8X16[code];
    for (row_idx, row_val) in glyph.iter().enumerate() {
        let py = y + row_idx;
        let offset = py * buf_width + x;
        for col_idx in 0..8 {
            if (row_val & (1 << (7 - col_idx))) != 0 {
                let cx = x + col_idx;
                if cx < buf_width {
                    let px = offset + col_idx;
                    if px < buffer.len() {
                        buffer[px] = color;
                    }
                }
            }
        }
    }
}

/// Desenha uma string com a fonte 8x16, limitando a largura máxima
pub fn draw_string(buffer: &mut [u32], buf_width: usize, x: usize, y: usize, text: &str, color: u32, max_width: usize) {
    let mut current_x = x;
    let char_width = 8;
    for c in text.chars() {
        if current_x + char_width > x + max_width {
            break;
        }
        draw_char(buffer, buf_width, current_x, y, c, color);
        current_x += char_width;
    }
}

/// Renderiza a barra de abas completa no buffer
pub fn render_tab_bar(buffer: &mut [u32], width: usize, tabs: &[Tab], active_index: usize) {
    let bg_color = 0xFF_11_11_11; // Darkest background (Title bar)
    let fg_color = 0xFF_D4_D4_D4; 
    let active_bg = 0xFF_28_28_28; // Matches Omnibox background
    let inactive_bg = 0xFF_1C_1C_1C; 
    
    // Title bar background
    clear_rect(buffer, width, 0, 0, width, TABBAR_HEIGHT as usize, bg_color);
    
    for (i, tab) in tabs.iter().enumerate() {
        let start_x = i * TAB_WIDTH + TAB_MARGIN_LEFT; // Margin from left
        if start_x >= width { break; } 
        
        let is_active = i == active_index;
        let t_bg = if is_active { active_bg } else { inactive_bg };
        
        let w = if start_x + TAB_WIDTH > width { width - start_x } else { TAB_WIDTH };
        
        // Tab shape: top rounded, bottom flat. We can draw it row by row or use clear_rect and carve.
        // We'll draw beveled top corners manually by drawing rectangles.
        let tab_y = 8; // Tabs are padded from the top
        let tab_h = TABBAR_HEIGHT as usize - tab_y; // Connects to the bottom bar
        
        // Draw the main tab block
        clear_rect(buffer, width, start_x + 2, tab_y, w - 4, tab_h, t_bg); // Body
        clear_rect(buffer, width, start_x + 1, tab_y + 1, w - 2, tab_h - 1, t_bg); // Mid bevel
        clear_rect(buffer, width, start_x, tab_y + 2, w, tab_h - 2, t_bg); // Full width base
        
        // Ícone minúsculo / Margem e Título
        let text_x = start_x + 16;
        let text_y = tab_y + (tab_h - 16) / 2; // Centralizado no bloco da aba
        
        let title_to_draw = if tab.title.is_empty() { "Nova Aba" } else { &tab.title };
        let active_fg = if is_active { 0xFF_FF_FF_FF } else { 0xFF_99_99_99 };
        
        draw_string(buffer, width, text_x, text_y, title_to_draw, active_fg, w.saturating_sub(40));
        
        // Botão X
        let close_x = start_x + w - 24;
        if close_x > start_x + 20 {
            draw_char(buffer, width, close_x, text_y, 'x', 0xFF_77_77_77);
        }
    }
    
    // Botão nova aba '+'
    let plus_x = tabs.len() * TAB_WIDTH + 16;
    if plus_x < width {
        let plus_y = 12;
        draw_char(buffer, width, plus_x, plus_y, '+', fg_color);
    }
}
