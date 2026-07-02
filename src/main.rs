mod engine;
mod fsm;
mod memory;
mod network;
mod ui;
mod config;

use crossbeam_channel::unbounded;
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::time::{Duration, Instant};
use winit::{
    event::{ElementState, Event, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey, ModifiersState},
    window::WindowBuilder,
};

fn load_icon() -> Option<winit::window::Icon> {
    let icon_bytes = include_bytes!("../Petal_icon.png");
    let image = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png).ok()?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    winit::window::Icon::from_rgba(rgba, width, height).ok()
}

#[cfg(target_os = "windows")]
fn force_focus_window(window: &winit::window::Window) {
    use winit::raw_window_handle::HasWindowHandle;
    if let Ok(handle) = window.window_handle() {
        if let winit::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
            unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h.hwnd.get() as _);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn force_focus_window(_window: &winit::window::Window) {
    // No-op for non-Windows platforms
}

fn ensure_active_webview(
    tab_manager: &mut fsm::tab_manager::TabManager,
    webviews: &mut std::collections::HashMap<u32, wry::WebView>,
    window: &winit::window::Window,
    adblock_engine: &network::adblock::AdblockEngine,
    hardware_acceleration: bool,
    ipc_tx: crossbeam_channel::Sender<String>,
) {
    loop {
        if let Some(active) = tab_manager.get_active_tab().cloned() {
            for (id, wv) in webviews.iter() {
                if *id != active.id {
                    wv.set_visible(false);
                }
            }

            if let Some(wv) = webviews.get(&active.id) {
                wv.set_visible(true);
                wv.focus();
                break;
            } else {
                match engine::builder::build_webview(
                    window,
                    adblock_engine,
                    &active.url,
                    active.id,
                    hardware_acceleration,
                    ipc_tx.clone(),
                ) {
                    Ok(new_wv) => {
                        new_wv.set_visible(true);
                        new_wv.focus();
                        webviews.insert(active.id, new_wv);
                        break;
                    }
                    Err(e) => {
                        eprintln!("FALHA FATAL EM ABA: Não foi possível recriar WebView para a aba ativa (ID {}): {}", active.id, e);
                        let idx = tab_manager.active_index;
                        if let Some(removed_id) = tab_manager.close_tab(idx) {
                            webviews.remove(&removed_id);
                        }
                        if tab_manager.tabs.is_empty() {
                            break;
                        }
                        continue;
                    }
                }
            }
        } else {
            break;
        }
    }
}

fn open_settings_window(
    settings_window: &mut Option<(winit::window::Window, wry::WebView)>,
    elwt: &winit::event_loop::EventLoopWindowTarget<()>,
    settings_tx: &crossbeam_channel::Sender<String>,
    browser_config: &config::BrowserConfig,
) {
    if settings_window.is_none() {
        let mut sw_builder = winit::window::WindowBuilder::new()
            .with_title("Configurações do Petal")
            .with_inner_size(winit::dpi::LogicalSize::new(450.0, 350.0));
        if let Some(icon) = load_icon() {
            sw_builder = sw_builder.with_window_icon(Some(icon));
        }
        match sw_builder.build(elwt) {
            Ok(sw) => {
                let tx = settings_tx.clone();
                match wry::WebViewBuilder::new(&sw)
                    .with_ipc_handler(move |request| {
                        let _ = tx.send(request);
                    })
                    .with_html(ui::settings::get_settings_html(browser_config))
                {
                    Ok(builder) => {
                        match builder.build() {
                            Ok(swv) => *settings_window = Some((sw, swv)),
                            Err(e) => eprintln!("Aviso: Falha ao construir WebView de configurações: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Aviso: Falha na montagem de HTML das configs: {:?}", e),
                }
            }
            Err(e) => eprintln!("Aviso: OS recusou criar janela auxiliar de Configs: {}", e),
        }
    }
}

fn main() {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("FALHA FATAL: Não foi possível instanciar o EventLoop do sistema: {}", e);
            std::process::exit(1);
        }
    };

    let mut window_builder = WindowBuilder::new()
        .with_title("Petal Browser [Bare-Metal Edition]")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));

    if let Some(icon) = load_icon() {
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    let window = match window_builder.build(&event_loop) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("FALHA FATAL: Falha arquitetural crítica ao criar a janela principal: {}", e);
            std::process::exit(1);
        }
    };

    let sb_context = match unsafe { Context::new(&window) } {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FALHA FATAL: Não foi possível criar contexto gráfico (Softbuffer). Sem suporte GPU/Display? Erro: {:?}", e);
            std::process::exit(1);
        }
    };

    let mut sb_surface = match unsafe { Surface::new(&sb_context, &window) } {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FALHA FATAL: Não foi possível criar superfície gráfica (Softbuffer). Erro: {:?}", e);
            std::process::exit(1);
        }
    };

    let adblock_engine = network::adblock::AdblockEngine::start();


    let mut tab_manager = fsm::tab_manager::TabManager::new();
    let mut os_trimmer = memory::os_trim::OsTrimmer::new();
    
    let mut browser_config = config::BrowserConfig::load();
    let mut omnibox = ui::omnibox::OmniboxState::new();
    let mut omnibox_layout = ui::omnibox::OmniboxLayout::new();

    let (ipc_tx, ipc_rx) = unbounded::<String>();
    let (settings_tx, settings_rx) = unbounded::<String>();

    let mut webviews = std::collections::HashMap::<u32, wry::WebView>::new();
    
    ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
    if tab_manager.tabs.is_empty() {
        eprintln!("AVISO: Nenhuma aba ativa na inicialização. Iniciando em estado vazio.");
    }
    let mut settings_window: Option<(winit::window::Window, wry::WebView)> = None;
    let mut modifiers = ModifiersState::empty();
    let mut cursor_x = 0.0;
    let mut cursor_y = 0.0;
    let mut dirty_region = ui::DirtyRegion::new();
    let mut compositor = ui::UICompositor::new();

    let mut redraw_pending = true;

    window.set_ime_allowed(true);
    dirty_region.invalidate_all();
    window.request_redraw();

    if let Err(e) = event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));

        match event {
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                if window_id == window.id() {
                    elwt.exit();
                } else {
                    settings_window = None;
                }
            }
            Event::WindowEvent { event: WindowEvent::Focused(focused), .. } => {
                if !focused {
                    omnibox.defocus();
                    dirty_region.invalidate_omnibox();
                }
            }
            Event::WindowEvent { event: WindowEvent::Ime(winit::event::Ime::Commit(text)), .. } => {
                if omnibox.is_focused {
                    omnibox.insert_str(&text);
                    dirty_region.invalidate_omnibox();
                }
            }
            Event::WindowEvent { event: WindowEvent::ModifiersChanged(mods), .. } => {
                modifiers = mods.state();
            }
            Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
                cursor_x = position.x;
                cursor_y = position.y;
            }
            Event::WindowEvent { event: WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }, .. } => {
                if cursor_y < ui::CHROME_HEIGHT as f64 {
                    window.focus_window();
                    force_focus_window(&window);
                }

                // Rastreio de cliques na barra de abas
                if cursor_y < ui::TABBAR_HEIGHT as f64 {
                    let hit = ui::hit_test_tab_bar(cursor_x, tab_manager.tabs.len(), window.inner_size().width as f64);
                    match hit {
                        ui::TabHit::CloseButton(clicked_index) => {
                            if let Some(removed_id) = tab_manager.close_tab(clicked_index) {
                                webviews.remove(&removed_id);
                            }
                            ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                            dirty_region.invalidate_chrome();
                        }
                        ui::TabHit::Tab(clicked_index) => {
                            if tab_manager.switch_tab(clicked_index) {
                                ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                                omnibox.defocus();
                                dirty_region.invalidate_chrome();
                            }
                        }
                        ui::TabHit::NewTabButton => {
                            tab_manager.new_tab("petal://newtab".to_string());
                            ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                            omnibox.defocus();
                            dirty_region.invalidate_chrome();
                        }
                        ui::TabHit::None => {}
                    }
                } else if cursor_y < ui::CHROME_HEIGHT as f64 {
                    let w = window.inner_size().width as usize;
                    match ui::omnibox::hit_test_omnibox(cursor_x, w) {
                        ui::omnibox::OmniboxHit::Back => {
                            if let Some(active) = tab_manager.get_active_tab() {
                                if let Some(wv) = webviews.get(&active.id) { let _ = wv.evaluate_script("window.history.back()"); }
                            }
                        }
                        ui::omnibox::OmniboxHit::Forward => {
                            if let Some(active) = tab_manager.get_active_tab() {
                                if let Some(wv) = webviews.get(&active.id) { let _ = wv.evaluate_script("window.history.forward()"); }
                            }
                        }
                        ui::omnibox::OmniboxHit::Refresh => {
                            if let Some(active) = tab_manager.get_active_tab() {
                                if let Some(wv) = webviews.get(&active.id) { let _ = wv.evaluate_script("location.reload()"); }
                            }
                        }
                        ui::omnibox::OmniboxHit::Settings => {
                            open_settings_window(&mut settings_window, elwt, &settings_tx, &browser_config);
                        }
                        ui::omnibox::OmniboxHit::Omnibox => {
                            if let Some(active) = tab_manager.get_active_tab() {
                                omnibox.focus(&active.url);
                                dirty_region.invalidate_omnibox();
                            }
                        }
                        ui::omnibox::OmniboxHit::None => {}
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, logical_key, text, .. }, .. }, .. } => {
                let ctrl = modifiers.control_key();
                let shift = modifiers.shift_key();
                
                if ctrl {
                    match logical_key.as_ref() {
                        Key::Character(",") => {
                            open_settings_window(&mut settings_window, elwt, &settings_tx, &browser_config);
                        }
                        Key::Character("l") | Key::Character("L") => {
                            if let Some(active) = tab_manager.get_active_tab() {
                                omnibox.focus(&active.url);
                                dirty_region.invalidate_omnibox();
                            }
                        }
                        Key::Character("t") | Key::Character("T") => {
                            tab_manager.new_tab("petal://newtab".to_string());
                            ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                            omnibox.defocus();
                            dirty_region.invalidate_chrome();
                        }
                        Key::Character("w") | Key::Character("W") => {
                            let idx = tab_manager.active_index;
                            if let Some(removed_id) = tab_manager.close_tab(idx) {
                                webviews.remove(&removed_id);
                            }
                            ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                            omnibox.defocus();
                            dirty_region.invalidate_chrome();
                        }
                        Key::Character("v") | Key::Character("V") => {
                            if omnibox.is_focused {
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    if let Ok(text) = clipboard.get_text() {
                                        let clean_text = text.replace('\n', "").replace('\r', "");
                                        omnibox.insert_str(&clean_text);
                                        dirty_region.invalidate_omnibox();
                                    }
                                }
                            }
                        }
                        Key::Named(NamedKey::Tab) => {
                            let mut next = tab_manager.active_index + 1;
                            if shift {
                                next = if tab_manager.active_index == 0 { tab_manager.tabs.len() - 1 } else { tab_manager.active_index - 1 };
                            } else if next >= tab_manager.tabs.len() {
                                next = 0;
                            }
                            if tab_manager.switch_tab(next) {
                                ensure_active_webview(&mut tab_manager, &mut webviews, &window, &adblock_engine, browser_config.hardware_acceleration, ipc_tx.clone());
                                omnibox.defocus();
                                dirty_region.invalidate_chrome();
                            }
                        }
                        _ => {}
                    }
                } else if omnibox.is_focused {
                    match logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            omnibox.defocus();
                            dirty_region.invalidate_omnibox();
                        }
                        Key::Named(NamedKey::Enter) => {
                            let final_url = ui::omnibox::resolve_navigation_target(&omnibox.input, &browser_config.search_engine);
                            if !final_url.is_empty() {
                                omnibox.push_history(final_url.clone());
                                if let Some(active_tab) = tab_manager.get_active_tab() {
                                    let active_id = active_tab.id;
                                    tab_manager.update_active_url(final_url.clone());
                                    if let Some(wv) = webviews.get(&active_id) {
                                        let _ = wv.load_url(&final_url);
                                    }
                                }
                            }
                            omnibox.defocus();
                            dirty_region.invalidate_chrome();
                        }
                        Key::Named(NamedKey::Backspace) => {
                            omnibox.backspace();
                            dirty_region.invalidate_omnibox();
                        }
                        Key::Named(NamedKey::Delete) => {
                            omnibox.delete();
                            dirty_region.invalidate_omnibox();
                        }
                        Key::Named(NamedKey::Home) => {
                            omnibox.home();
                            dirty_region.invalidate_omnibox();
                        }
                        Key::Named(NamedKey::End) => {
                            omnibox.end();
                            dirty_region.invalidate_omnibox();
                        }
                        Key::Named(NamedKey::ArrowLeft) => { omnibox.arrow_left(); dirty_region.invalidate_omnibox(); }
                        Key::Named(NamedKey::ArrowRight) => { omnibox.arrow_right(); dirty_region.invalidate_omnibox(); }
                        Key::Named(NamedKey::ArrowUp) => { omnibox.arrow_up(); dirty_region.invalidate_omnibox(); }
                        Key::Named(NamedKey::ArrowDown) => { omnibox.arrow_down(); dirty_region.invalidate_omnibox(); }
                        Key::Named(NamedKey::Space) => {
                            omnibox.insert_char(' ');
                            dirty_region.invalidate_omnibox();
                        }
                        _ => {
                            let mut handled = false;
                            if let Some(t) = text {
                                let mut to_insert = String::with_capacity(t.len());
                                for c in t.chars() {
                                    if !c.is_control() {
                                        to_insert.push(c);
                                    }
                                }
                                if !to_insert.is_empty() {
                                    omnibox.insert_str(&to_insert);
                                    handled = true;
                                }
                            }
                            if !handled {
                                if let Key::Character(c) = logical_key {
                                    if let Some(ch) = c.chars().next() {
                                        if !ch.is_control() {
                                            omnibox.insert_char(ch);
                                            handled = true;
                                        }
                                    }
                                }
                            }
                            if handled {
                                dirty_region.invalidate_omnibox();
                            }
                        }
                    }
                }
            }
            Event::WindowEvent { window_id, event: WindowEvent::Resized(size), .. } => {
                if size.width > 0 && size.height > 0 {
                    if window_id == window.id() {
                        let _ = sb_surface.resize(
                            NonZeroU32::new(size.width).unwrap_or(NonZeroU32::MIN),
                            NonZeroU32::new(size.height).unwrap_or(NonZeroU32::MIN),
                        );
                        dirty_region.invalidate_all();
                        window.request_redraw();
                        
                        for wv in webviews.values() {
                            let bounds = wry::Rect {
                                x: 0,
                                y: ui::CHROME_HEIGHT as i32,
                                width: size.width,
                                height: size.height.saturating_sub(ui::CHROME_HEIGHT),
                            };
                            let _ = wv.set_bounds(bounds);
                        }
                    } else if let Some((sw, swv)) = settings_window.as_ref() {
                        if window_id == sw.id() {
                            let _ = swv.set_bounds(wry::Rect {
                                x: 0,
                                y: 0,
                                width: size.width,
                                height: size.height,
                            });
                        }
                    }
                }
            }
            Event::WindowEvent { window_id, event: WindowEvent::RedrawRequested, .. } => {
                if window_id == window.id() {
                    redraw_pending = false;
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        match sb_surface.buffer_mut() {
                            Ok(mut buffer) => {
                                let w = size.width as usize;
                                let h = size.height as usize;
                                let expected_len = w.checked_mul(h);

                                match expected_len {
                                    Some(len) if len == buffer.len() => {

                                        // 1. Manutenção de Camadas (LayerCaches)
                                        let tabbar_h = ui::TABBAR_HEIGHT as usize;
                                        let omnibox_h = ui::OMNIBOX_HEIGHT as usize;
                                        let chrome_height = ui::CHROME_HEIGHT as usize;

                                        compositor.tabbar.ensure_size(w, tabbar_h);
                                        compositor.static_omnibox.ensure_size(w, omnibox_h);

                                        if dirty_region.tabbar || dirty_region.whole_window {
                                            compositor.tabbar.invalidate();
                                        }

                                        if dirty_region.omnibox || dirty_region.whole_window || compositor.omnibox_is_focused != omnibox.is_focused {
                                            compositor.static_omnibox.invalidate();
                                            compositor.omnibox_is_focused = omnibox.is_focused;
                                        }

                                        // 2. Repintar as Camadas (apenas se invalidadas de forma independente)
                                        if !compositor.tabbar.valid {
                                            ui::render_tab_bar(&mut compositor.tabbar.buffer, w, &tab_manager.tabs, tab_manager.active_index);
                                            compositor.tabbar.valid = true;
                                        }
                                        if !compositor.static_omnibox.valid {
                                            ui::omnibox::render_omnibox_static(&mut compositor.static_omnibox.buffer, w, omnibox.is_focused);
                                            compositor.static_omnibox.valid = true;
                                        }

                                        // 3. Composição (Blitting iterativo para o Frame final)
                                        if dirty_region.whole_window {
                                            buffer.fill(0xFF_12_12_12); // Limpa o resto da tela
                                        }

                                        if dirty_region.tabbar || dirty_region.whole_window {
                                            let cache_len = w * tabbar_h;
                                            if buffer.len() >= cache_len && compositor.tabbar.buffer.len() >= cache_len {
                                                buffer[..cache_len].copy_from_slice(&compositor.tabbar.buffer[..cache_len]);
                                            }
                                        }

                                        if dirty_region.omnibox || dirty_region.whole_window {
                                            let start = w * tabbar_h;
                                            let end = w * chrome_height;
                                            let cache_len = w * omnibox_h;
                                            if buffer.len() >= end && compositor.static_omnibox.buffer.len() >= cache_len {
                                                buffer[start..end].copy_from_slice(&compositor.static_omnibox.buffer[..cache_len]);
                                            }
                                        }

                                        
                                        // 4. Desenho Dinâmico
                                        if dirty_region.omnibox || dirty_region.tabbar || dirty_region.whole_window {
                                            let url_to_display = tab_manager.get_active_tab().map(|t| t.url.as_str()).unwrap_or("");
                                            omnibox_layout.update(&mut omnibox, w, url_to_display);
                                            ui::omnibox::render_omnibox_dynamic(&mut buffer, w, &omnibox_layout);
                                        }
                                        
                                        let _ = buffer.present();
                                        dirty_region.reset();
                                    }
                                    _ => {
                                        eprintln!("AVISO: Redraw ignorado devido a discrepância do buffer. Real: {}, Esperado: {:?}", buffer.len(), expected_len);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("AVISO: Falha ao obter buffer gráfico do Softbuffer para redraw: {:?}", e);
                            }
                        }
                    }
                    
                    #[cfg(target_os = "windows")]
                    {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
                        use winit::raw_window_handle::HasWindowHandle;
                        if let Ok(handle) = window.window_handle() {
                            let raw = handle.as_raw();
                            let winit_hwnd = match raw {
                                winit::raw_window_handle::RawWindowHandle::Win32(h) => h.hwnd.get() as _,
                                _ => 0,
                            };
                            if winit_hwnd != 0 {
                                let mut child = unsafe { FindWindowExW(winit_hwnd as _, 0, std::ptr::null(), std::ptr::null()) };
                                while child != 0 {
                                    let size = window.inner_size();
                                    if size.height > ui::CHROME_HEIGHT {
                                        unsafe { SetWindowPos(child, 0, 0, ui::CHROME_HEIGHT as i32, size.width as i32, size.height.saturating_sub(ui::CHROME_HEIGHT) as i32, SWP_NOZORDER | SWP_NOACTIVATE); }
                                    }
                                    child = unsafe { FindWindowExW(winit_hwnd as _, child, std::ptr::null(), std::ptr::null()) };
                                }
                            }
                        }
                    }
                }
            }
            Event::AboutToWait => {
                // Checar IPC Wry das abas normais
                while let Ok(msg) = ipc_rx.try_recv() {
                    let parts: Vec<&str> = msg.splitn(3, '|').collect();
                    if parts.len() == 3 {
                        if let Ok(tab_id) = parts[0].parse::<u32>() {
                            let cmd = parts[1];
                            let payload = parts[2].to_string();
                            if cmd == "title" {
                                if tab_manager.update_tab_title(tab_id, payload) {
                                    dirty_region.invalidate_tabbar();
                                } else {
                                    eprintln!("AVISO: Comando IPC (title) recebido para aba inativa/removida: {}", tab_id);
                                }
                            } else if cmd == "url" {
                                if tab_manager.update_tab_url(tab_id, payload) {
                                    dirty_region.invalidate_omnibox();
                                } else {
                                    eprintln!("AVISO: Comando IPC (url) recebido para aba inativa/removida: {}", tab_id);
                                }
                            } else if cmd == "focus_omnibox" {
                                window.focus_window();
                                force_focus_window(&window);
                                if let Some(active) = tab_manager.get_active_tab() {
                                    omnibox.focus(&active.url);
                                }
                                dirty_region.invalidate_omnibox();
                            }
                        }
                    } else {
                        eprintln!("AVISO DE SEGURANÇA: Aba tentou enviar um formato IPC suspeito ou não autorizado: {}", msg);
                    }
                }

                // Checar IPC Wry privilegiado (Janela de Configurações)
                while let Ok(msg) = settings_rx.try_recv() {
                    if let Some(payload) = msg.strip_prefix("save_config:") {
                        if let Ok(mut new_config) = serde_json::from_str::<crate::config::BrowserConfig>(payload) {
                            new_config.validate();
                            let hw = new_config.hardware_acceleration;
                            let search = new_config.search_engine;

                            let hw_changed = hw != browser_config.hardware_acceleration;
                            browser_config.hardware_acceleration = hw;
                            browser_config.search_engine = search;
                            if let Err(e) = browser_config.save() {
                                println!("Erro crítico ao salvar preferências: {}", e);
                            }
                            settings_window = None;
                            if hw_changed {
                                println!("⚠️ Aceleração de Hardware alterada. Reinicie o navegador para aplicar.");
                            }
                        }
                    }
                }
                
                // Ostrimmer
                let active_wv = tab_manager.get_active_tab().map(|t| t.id).and_then(|id| webviews.get(&id));
                if let Ok(memory::os_trim::TrimAction::EmergencyFallback) = os_trimmer.try_trim(active_wv) {
                    if let Some(wv) = active_wv {
                        let _ = wv.load_url("petal://newtab");
                    }
                }

                // Cursor blink
                if omnibox.is_focused {
                    if omnibox_layout.last_cursor_blink.elapsed().as_millis() >= 500 {
                        omnibox_layout.cursor_blink_visible = !omnibox_layout.cursor_blink_visible;
                        omnibox_layout.last_cursor_blink = std::time::Instant::now();
                        dirty_region.invalidate_omnibox();
                    }
                }

                // Redraw coalescido: um único request_redraw para todos os eventos pendentes
                if dirty_region.needs_redraw() && !redraw_pending {
                    window.request_redraw();
                    redraw_pending = true;
                }
            }
            _ => (),
        }
    }) {
        eprintln!("O loop de eventos principal encerrou de forma anormal: {}", e);
    }
}


