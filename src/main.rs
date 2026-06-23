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
    keyboard::{Key, NamedKey, PhysicalKey, ModifiersState},
    window::WindowBuilder,
};

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let window = WindowBuilder::new()
        .with_title("Magma Browser [Bare-Metal Edition]")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
        .build(&event_loop)
        .expect("Falha arquitetural crítica.");

    let sb_context = unsafe { Context::new(&window).expect("Falha Softbuffer") };
    let mut sb_surface = unsafe { Surface::new(&sb_context, &window).expect("Falha Softbuffer") };

    let adblock_engine = network::adblock::AdblockEngine::start();
    let ephemeral_context = engine::builder::EphemeralWebContext::new();

    let mut tab_manager = fsm::tab_manager::TabManager::new();
    let mut os_trimmer = memory::os_trim::OsTrimmer::new();
    
    let mut browser_config = config::BrowserConfig::load();
    let mut omnibox = ui::omnibox::OmniboxState::new();

    let (ipc_tx, ipc_rx) = unbounded::<String>();

    let mut _webview = Some(engine::builder::build_webview(
        &window,
        &ephemeral_context,
        &adblock_engine,
        &tab_manager.get_active_tab().unwrap().url,
        browser_config.hardware_acceleration,
        ipc_tx.clone(),
    ).expect("Falha ao instanciar WebView."));

    let mut settings_window: Option<(winit::window::Window, wry::WebView)> = None;

    let adblock_engine_loop = adblock_engine.clone();

    let mut modifiers = ModifiersState::empty();
    let mut cursor_x = 0.0;
    let mut cursor_y = 0.0;

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16)));

        match event {
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                if window_id == window.id() {
                    elwt.exit();
                } else {
                    settings_window = None;
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
                // Rastreio de cliques na barra de abas
                if cursor_y < ui::TABBAR_HEIGHT as f64 {
                    let tab_width = 200.0;
                    let clicked_index = (cursor_x / tab_width).floor() as usize;
                    
                    if clicked_index < tab_manager.tabs.len() {
                        // Verifica se clicou no X de fechar (últimos 20px)
                        let relative_x = cursor_x % tab_width;
                        if relative_x > tab_width - 25.0 {
                            tab_manager.close_tab(clicked_index);
                            _webview = None; // Dropa instantaneamente
                            _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                        } else {
                            // Troca de Aba (Regra de Ouro: Drop e Reidratação)
                            if tab_manager.switch_tab(clicked_index) {
                                _webview = None;
                                _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                                omnibox.defocus();
                            }
                        }
                        window.request_redraw();
                    } else if clicked_index == tab_manager.tabs.len() {
                        // Clicou no botão +
                        tab_manager.new_tab("https://magma.browser/local_cache".to_string());
                        _webview = None;
                        _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                        omnibox.defocus();
                        window.request_redraw();
                    }
                } else if cursor_y < ui::CHROME_HEIGHT as f64 {
                    omnibox.focus(&tab_manager.get_active_tab().unwrap().url);
                    window.request_redraw();
                }
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, logical_key, physical_key, .. }, .. }, .. } => {
                let ctrl = modifiers.control_key();
                let shift = modifiers.shift_key();
                
                if ctrl {
                    match physical_key {
                        PhysicalKey::Code(winit::keyboard::KeyCode::Comma) => {
                            if settings_window.is_none() {
                                let sw = WindowBuilder::new()
                                    .with_title("Configurações do Magma")
                                    .with_inner_size(winit::dpi::LogicalSize::new(450.0, 350.0))
                                    .build(elwt)
                                    .unwrap();
                                let tx = ipc_tx.clone();
                                let swv = wry::WebViewBuilder::new(&sw)
                                    .with_ipc_handler(move |request| {
                                        let _ = tx.send(request);
                                    })
                                    .with_html(ui::settings::get_settings_html(&browser_config))
                                    .unwrap()
                                    .build()
                                    .unwrap();
                                settings_window = Some((sw, swv));
                            }
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::KeyL) => {
                            omnibox.focus(&tab_manager.get_active_tab().unwrap().url);
                            window.request_redraw();
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::KeyT) => {
                            tab_manager.new_tab("https://magma.browser/local_cache".to_string());
                            _webview = None;
                            _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::KeyW) => {
                            let idx = tab_manager.active_index;
                            tab_manager.close_tab(idx);
                            _webview = None;
                            _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::Tab) => {
                            let mut next = tab_manager.active_index + 1;
                            if shift {
                                next = if tab_manager.active_index == 0 { tab_manager.tabs.len() - 1 } else { tab_manager.active_index - 1 };
                            } else if next >= tab_manager.tabs.len() {
                                next = 0;
                            }
                            if tab_manager.switch_tab(next) {
                                _webview = None;
                                _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                                omnibox.defocus();
                                window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                } else if omnibox.is_focused {
                    match logical_key.as_ref() {
                        Key::Named(NamedKey::Escape) => {
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::Enter) => {
                            let final_url = ui::omnibox::resolve_navigation_target(&omnibox.input, &browser_config.search_engine);
                            if !final_url.is_empty() {
                                omnibox.push_history(final_url.clone());
                                tab_manager.update_active_url(final_url.clone());
                                if let Some(wv) = _webview.as_ref() {
                                    let _ = wv.load_url(&final_url);
                                }
                            }
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::Backspace) => {
                            omnibox.backspace();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::ArrowLeft) => { omnibox.arrow_left(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowRight) => { omnibox.arrow_right(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowUp) => { omnibox.arrow_up(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowDown) => { omnibox.arrow_down(); window.request_redraw(); }
                        Key::Character(c) => {
                            if let Some(ch) = c.chars().next() {
                                omnibox.insert_char(ch);
                                window.request_redraw();
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                if size.width > 0 && size.height > 0 {
                    let _ = sb_surface.resize(
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    );
                    window.request_redraw();
                    
                    if let Some(wv) = _webview.as_ref() {
                        let bounds = wry::Rect {
                            x: 0,
                            y: ui::CHROME_HEIGHT as i32,
                            width: size.width,
                            height: size.height.saturating_sub(ui::CHROME_HEIGHT),
                        };
                        let _ = wv.set_bounds(bounds);
                    }
                }
            }
            Event::WindowEvent { window_id, event: WindowEvent::RedrawRequested, .. } => {
                if window_id == window.id() {
                    let size = window.inner_size();
                    if size.width > 0 && size.height > 0 {
                        if let Ok(mut buffer) = sb_surface.buffer_mut() {
                            // Limpa a tela inteira (incluindo onde os snapshots vao depois da TabBar)
                            for index in 0..(size.width * size.height) {
                                buffer[index as usize] = 0xFF_12_12_12; 
                            }
                            
                            // Renderiza barra de abas
                            ui::render_tab_bar(&mut buffer, size.width as usize, &tab_manager.tabs, tab_manager.active_index);
                            ui::omnibox::render_omnibox(&mut buffer, size.width as usize, &omnibox, &tab_manager.get_active_tab().unwrap().url);
                            
                            let _ = buffer.present();
                        }
                    }
                }
            }
            Event::AboutToWait => {
                // Checar IPC Wry
                while let Ok(msg) = ipc_rx.try_recv() {
                    if let Some(title) = msg.strip_prefix("title:") {
                        let new_title = title.to_string();
                        let active = tab_manager.get_active_tab().unwrap();
                        if active.title != new_title {
                            tab_manager.update_active_title(new_title);
                            window.request_redraw();
                        }
                    } else if let Some(url) = msg.strip_prefix("url:") {
                        let new_url = url.to_string();
                        let active = tab_manager.get_active_tab().unwrap();
                        if active.url != new_url {
                            tab_manager.update_active_url(new_url);
                            window.request_redraw();
                        }
                    } else if let Some(payload) = msg.strip_prefix("save_config:") {
                        let mut hw = browser_config.hardware_acceleration;
                        let mut search = browser_config.search_engine.clone();
                        for part in payload.split('|') {
                            if let Some((k, v)) = part.split_once('=') {
                                if k == "hw" { hw = v == "true"; }
                                else if k == "search" { search = v.replace("%7C", "|"); }
                            }
                        }
                        let hw_changed = hw != browser_config.hardware_acceleration;
                        browser_config.hardware_acceleration = hw;
                        browser_config.search_engine = search;
                        browser_config.save();
                        settings_window = None;
                        if hw_changed {
                            println!("⚠️ Aceleração de Hardware alterada. Reinicie o navegador para aplicar.");
                        }
                    }
                }
                
                // Ostrimmer
                if let Ok(memory::os_trim::TrimAction::EmergencyCrash) = os_trimmer.try_trim(_webview.as_ref()) {
                    _webview = None;
                    _webview = Some(engine::builder::build_webview(&window, &ephemeral_context, &adblock_engine, &tab_manager.get_active_tab().unwrap().url, browser_config.hardware_acceleration, ipc_tx.clone()).expect("Falha"));
                }
            }
            _ => (),
        }
    }).unwrap();
}
