use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use winit::window::Window;
use wry::{Rect, WebView, WebViewBuilder};
#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;
use crate::network::adblock::AdblockEngine;

pub struct EphemeralWebContext {
    pub data_dir: PathBuf,
}

impl EphemeralWebContext {
    pub fn new() -> Self {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let mut data_dir = env::temp_dir();
        data_dir.push(format!("magma_volatile_{}", timestamp));
        fs::create_dir_all(&data_dir).expect("Falha");
        Self { data_dir }
    }
}

impl Drop for EphemeralWebContext {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.data_dir);
    }
}

pub fn build_webview(
    window: &Window,
    _ephemeral_context: &EphemeralWebContext,
    adblock_engine: &AdblockEngine,
    url: &str,
    tab_id: u32,
    hardware_acceleration: bool,
    ipc_tx: crossbeam_channel::Sender<String>,
) -> wry::Result<WebView> {
    let mut builder = WebViewBuilder::new(window);

    // Constranger o WebView para deixar a área da barra de abas visível no host Winit
    let size = window.inner_size();
    if size.height > crate::ui::CHROME_HEIGHT {
        let bounds = Rect {
            x: 0,
            y: crate::ui::CHROME_HEIGHT as i32,
            width: size.width,
            height: size.height - crate::ui::CHROME_HEIGHT,
        };
        builder = builder.with_bounds(bounds);
    }

    let tx_nav = ipc_tx.clone();
    builder = builder.with_on_page_load_handler(move |event, url| {
        // Envia apenas quando o carregamento termina ou muda
        if let wry::PageLoadEvent::Finished = event {
            let _ = tx_nav.send(format!("{}|url|{}", tab_id, url));
        }
    });

    // Injeção de IPC para rastrear Document Title (nativo não suportado cross-platform sem extensões)
    builder = builder.with_ipc_handler(move |request| {
        let msg = request; // request is a String in wry
        let _ = ipc_tx.send(msg);
    });

    let init_script = format!(r#"
        (function() {{
            window.ipc.postMessage('{}|title|' + document.title);
            new MutationObserver(function(mutations) {{
                window.ipc.postMessage('{}|title|' + document.title);
            }}).observe(
                document.querySelector('title') || document.head,
                {{ subtree: true, characterData: true, childList: true }}
            );
        }})();
    "#, tab_id, tab_id);
    builder = builder.with_initialization_script(&init_script);

    #[cfg(target_os = "windows")]
    {
        let mut args = "--js-flags=\"--lite-mode --max-old-space-size=128 --scavenger_max_new_space_capacity_mb=4\" --renderer-process-limit=2".to_string();
        if !hardware_acceleration {
            args.push_str(" --disable-gpu");
        }
        builder = builder.with_additional_browser_args(&args);
    }
    
    let builder = adblock_engine.attach_to_builder(builder);
    builder.with_url(url)?.build()
}
