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
    for row in y..(y + h) {
        let row_base = row * buf_width;
        for col in 0..w {
            let px = x + col;
            // FIX: interrompe ao ultrapassar a largura do buffer, evitando "row-wrap" visual
            // (sem essa guarda, px >= buf_width causaria escrita na linha seguinte do buffer)
            if px >= buf_width { break; }
            let idx = row_base + px;
            if idx < buffer.len() {
                buffer[idx] = color;
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
            end_col = w - 1; // seguro: w >= 2 garantido pela guarda acima
        }

        let row_base = py * buf_width;
        for col in start_col..end_col {
            let px = x + col;
            // FIX: mesma guarda de row-wrap que clear_rect
            if px >= buf_width { break; }
            let offset = row_base + px;
            if offset < buffer.len() {
                buffer[offset] = color;
            }
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

    let mut code = c as usize;
    if code >= 256 {
        code = '?' as usize;
    }
    let glyph = &font::FONT_8X16[code];

    for (row_idx, row_val) in glyph.iter().enumerate() {
        let py = y.saturating_add(row_idx);

        // Vertical clipping
        if py < clip_y || py >= clip_y.saturating_add(clip_h) || py >= buf_height {
            continue;
        }

        let offset = py.saturating_mul(buf_width);

        for col_idx in 0..8 {
            if (row_val & (1 << (7 - col_idx))) != 0 {
                let px = x.saturating_add(col_idx);

                // Horizontal clipping
                if px < clip_x || px >= clip_x.saturating_add(clip_w) || px >= buf_width {
                    continue;
                }

                let final_idx = offset.saturating_add(px);
                if final_idx < buffer.len() {
                    buffer[final_idx] = color;
                }
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