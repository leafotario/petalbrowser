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

fn load_icon() -> Option<winit::window::Icon> {
    let icon_bytes = include_bytes!("../Petal_icon.png");
    let image = image::load_from_memory_with_format(icon_bytes, image::ImageFormat::Png).ok()?;
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    winit::window::Icon::from_rgba(rgba, width, height).ok()
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let mut window_builder = WindowBuilder::new()
        .with_title("Petal Browser [Bare-Metal Edition]")
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0));

    if let Some(icon) = load_icon() {
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    let window = window_builder
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

    let mut webviews = std::collections::HashMap::<u32, wry::WebView>::new();
    let initial_tab = tab_manager.get_active_tab().unwrap();
    let initial_wv = engine::builder::build_webview(
        &window,
        &ephemeral_context,
        &adblock_engine,
        &initial_tab.url,
        initial_tab.id,
        browser_config.hardware_acceleration,
        ipc_tx.clone(),
    ).expect("Falha ao instanciar WebView.");
    webviews.insert(initial_tab.id, initial_wv);
    let mut settings_window: Option<(winit::window::Window, wry::WebView)> = None;
    let mut modifiers = ModifiersState::empty();
    let mut cursor_x = 0.0;
    let mut cursor_y = 0.0;

    window.set_ime_allowed(true);
    window.request_redraw();

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
            Event::WindowEvent { event: WindowEvent::Focused(focused), .. } => {
                if !focused {
                    omnibox.defocus();
                    window.request_redraw();
                }
            }
            Event::WindowEvent { event: WindowEvent::Ime(winit::event::Ime::Commit(text)), .. } => {
                if omnibox.is_focused {
                    omnibox.insert_str(&text);
                    window.request_redraw();
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
                    use winit::raw_window_handle::HasWindowHandle;
                    if let Ok(handle) = window.window_handle() {
                        if let winit::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                            unsafe {
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h.hwnd.get() as _);
                            }
                        }
                    }
                }

                // Rastreio de cliques na barra de abas
                if cursor_y < ui::TABBAR_HEIGHT as f64 {
                    let hit = ui::hit_test_tab_bar(cursor_x, tab_manager.tabs.len(), window.inner_size().width as f64);
                    match hit {
                        ui::TabHit::CloseButton(clicked_index) => {
                            if let Some(removed_id) = tab_manager.close_tab(clicked_index) {
                                webviews.remove(&removed_id);
                            }
                            let active = tab_manager.get_active_tab().unwrap();
                            if let Some(wv) = webviews.get(&active.id) {
                                wv.set_visible(true);
                                wv.focus();
                            } else {
                                let new_wv = engine::builder::build_webview(
                                    &window,
                                    &ephemeral_context,
                                    &adblock_engine,
                                    &active.url,
                                    active.id,
                                    browser_config.hardware_acceleration,
                                    ipc_tx.clone(),
                                ).unwrap();
                                webviews.insert(active.id, new_wv);
                            }
                            window.request_redraw();
                        }
                        ui::TabHit::Tab(clicked_index) => {
                            let old_active = tab_manager.get_active_tab().unwrap().id;
                            if tab_manager.switch_tab(clicked_index) {
                                let new_active = tab_manager.get_active_tab().unwrap().id;
                                if let Some(wv) = webviews.get(&old_active) {
                                    wv.set_visible(false);
                                }
                                if let Some(wv) = webviews.get(&new_active) {
                                    wv.set_visible(true);
                                    wv.focus();
                                }
                                omnibox.defocus();
                                window.request_redraw();
                            }
                        }
                        ui::TabHit::NewTabButton => {
                            tab_manager.new_tab("https://petal.browser/local_cache".to_string());
                            let new_tab = tab_manager.get_active_tab().unwrap();
                            let new_wv = engine::builder::build_webview(
                                &window,
                                &ephemeral_context,
                                &adblock_engine,
                                &new_tab.url,
                                new_tab.id,
                                browser_config.hardware_acceleration,
                                ipc_tx.clone(),
                            ).unwrap();
                            for wv in webviews.values() {
                                wv.set_visible(false);
                            }
                            webviews.insert(new_tab.id, new_wv);
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        ui::TabHit::None => {}
                    }
                } else if cursor_y < ui::CHROME_HEIGHT as f64 {
                    let w = window.inner_size().width as f64;
                    if cursor_x >= 10.0 && cursor_x < 46.0 {
                        // Back
                        let active_id = tab_manager.get_active_tab().unwrap().id;
                        if let Some(wv) = webviews.get(&active_id) { let _ = wv.evaluate_script("window.history.back()"); }
                    } else if cursor_x >= 52.0 && cursor_x < 88.0 {
                        // Forward
                        let active_id = tab_manager.get_active_tab().unwrap().id;
                        if let Some(wv) = webviews.get(&active_id) { let _ = wv.evaluate_script("window.history.forward()"); }
                    } else if cursor_x >= 94.0 && cursor_x < 130.0 {
                        // Refresh
                        let active_id = tab_manager.get_active_tab().unwrap().id;
                        if let Some(wv) = webviews.get(&active_id) { let _ = wv.evaluate_script("location.reload()"); }
                    } else if cursor_x > w - 46.0 {
                        // Settings
                        if settings_window.is_none() {
                            let mut sw_builder = WindowBuilder::new()
                                .with_title("Configurações do Petal")
                                .with_inner_size(winit::dpi::LogicalSize::new(450.0, 350.0));
                            if let Some(icon) = load_icon() {
                                sw_builder = sw_builder.with_window_icon(Some(icon));
                            }
                            let sw = sw_builder
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
                    } else {
                        // Omnibox Click
                        omnibox.focus(&tab_manager.get_active_tab().unwrap().url);
                        window.request_redraw();
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, logical_key, physical_key, text, .. }, .. }, .. } => {
                let ctrl = modifiers.control_key();
                let shift = modifiers.shift_key();
                
                if ctrl {
                    match physical_key {
                        PhysicalKey::Code(winit::keyboard::KeyCode::Comma) => {
                            if settings_window.is_none() {
                                let mut sw_builder = WindowBuilder::new()
                                    .with_title("Configurações do Petal")
                                    .with_inner_size(winit::dpi::LogicalSize::new(450.0, 350.0));
                                if let Some(icon) = load_icon() {
                                    sw_builder = sw_builder.with_window_icon(Some(icon));
                                }
                                let sw = sw_builder
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
                            tab_manager.new_tab("https://petal.browser/local_cache".to_string());
                            let new_tab = tab_manager.get_active_tab().unwrap();
                            let new_wv = engine::builder::build_webview(
                                &window,
                                &ephemeral_context,
                                &adblock_engine,
                                &new_tab.url,
                                new_tab.id,
                                browser_config.hardware_acceleration,
                                ipc_tx.clone(),
                            ).unwrap();
                            for wv in webviews.values() {
                                wv.set_visible(false);
                            }
                            webviews.insert(new_tab.id, new_wv);
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::KeyW) => {
                            let idx = tab_manager.active_index;
                            if let Some(removed_id) = tab_manager.close_tab(idx) {
                                webviews.remove(&removed_id);
                            }
                            let active = tab_manager.get_active_tab().unwrap();
                            if let Some(wv) = webviews.get(&active.id) {
                                wv.set_visible(true);
                                wv.focus();
                            } else {
                                let new_wv = engine::builder::build_webview(
                                    &window,
                                    &ephemeral_context,
                                    &adblock_engine,
                                    &active.url,
                                    active.id,
                                    browser_config.hardware_acceleration,
                                    ipc_tx.clone(),
                                ).unwrap();
                                webviews.insert(active.id, new_wv);
                            }
                            omnibox.defocus();
                            window.request_redraw();
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) => {
                            if omnibox.is_focused {
                                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                    if let Ok(text) = clipboard.get_text() {
                                        let clean_text = text.replace('\n', "").replace('\r', "");
                                        omnibox.insert_str(&clean_text);
                                        window.request_redraw();
                                    }
                                }
                            }
                        }
                        PhysicalKey::Code(winit::keyboard::KeyCode::Tab) => {
                            let mut next = tab_manager.active_index + 1;
                            if shift {
                                next = if tab_manager.active_index == 0 { tab_manager.tabs.len() - 1 } else { tab_manager.active_index - 1 };
                            } else if next >= tab_manager.tabs.len() {
                                next = 0;
                            }
                            let old_active = tab_manager.get_active_tab().unwrap().id;
                            if tab_manager.switch_tab(next) {
                                let new_active = tab_manager.get_active_tab().unwrap().id;
                                if let Some(wv) = webviews.get(&old_active) {
                                    wv.set_visible(false);
                                }
                                if let Some(wv) = webviews.get(&new_active) {
                                    wv.set_visible(true);
                                    wv.focus();
                                }
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
                                let active_id = tab_manager.get_active_tab().unwrap().id;
                                tab_manager.update_active_url(final_url.clone());
                                if let Some(wv) = webviews.get(&active_id) {
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
                        Key::Named(NamedKey::Delete) => {
                            omnibox.delete();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::Home) => {
                            omnibox.home();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::End) => {
                            omnibox.end();
                            window.request_redraw();
                        }
                        Key::Named(NamedKey::ArrowLeft) => { omnibox.arrow_left(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowRight) => { omnibox.arrow_right(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowUp) => { omnibox.arrow_up(); window.request_redraw(); }
                        Key::Named(NamedKey::ArrowDown) => { omnibox.arrow_down(); window.request_redraw(); }
                        Key::Named(NamedKey::Space) => {
                            omnibox.insert_char(' ');
                            window.request_redraw();
                        }
                        _ => {
                            let mut handled = false;
                            if let Some(t) = text {
                                for c in t.chars() {
                                    if !c.is_control() {
                                        omnibox.insert_char(c);
                                        handled = true;
                                    }
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
                                window.request_redraw();
                            }
                        }
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
                    
                    for wv in webviews.values() {
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
                            ui::omnibox::render_omnibox(&mut buffer, size.width as usize, &mut omnibox, &tab_manager.get_active_tab().unwrap().url);
                            
                            let _ = buffer.present();
                        }
                    }
                    
                    #[cfg(target_os = "windows")]
                    {
                        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
                        use winit::raw_window_handle::HasWindowHandle;
                        let raw = window.window_handle().unwrap().as_raw();
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
            Event::AboutToWait => {
                // Checar IPC Wry
                while let Ok(msg) = ipc_rx.try_recv() {
                    let parts: Vec<&str> = msg.splitn(3, '|').collect();
                    if parts.len() == 3 {
                        if let Ok(tab_id) = parts[0].parse::<u32>() {
                            let cmd = parts[1];
                            let payload = parts[2].to_string();
                            if cmd == "title" {
                                tab_manager.update_tab_title(tab_id, payload);
                                window.request_redraw();
                            } else if cmd == "url" {
                                tab_manager.update_tab_url(tab_id, payload);
                                window.request_redraw();
                            } else if cmd == "focus_omnibox" {
                                window.focus_window();
                                use winit::raw_window_handle::HasWindowHandle;
                                if let Ok(handle) = window.window_handle() {
                                    if let winit::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                                        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(h.hwnd.get() as _); }
                                    }
                                }
                                omnibox.focus(&tab_manager.get_active_tab().unwrap().url);
                                window.request_redraw();
                            }
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
                        if let Err(e) = browser_config.save() {
                            println!("Erro crítico ao salvar preferências: {}", e);
                        }
                        settings_window = None;
                        if hw_changed {
                            println!("⚠️ Aceleração de Hardware alterada. Reinicie o navegador para aplicar.");
                        }
                    }
                }
                
                // Ostrimmer
                let active_wv = webviews.get(&tab_manager.get_active_tab().unwrap().id);
                if let Ok(memory::os_trim::TrimAction::EmergencyCrash) = os_trimmer.try_trim(active_wv) {
                    if let Some(wv) = active_wv {
                        let _ = wv.load_url("https://petal.browser/local_cache");
                    }
                }
            }
            _ => (),
        }
    }).unwrap();
}



