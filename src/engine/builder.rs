use crate::network::adblock::AdblockEngine;
use winit::window::Window;
#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;
use wry::{Rect, WebView, WebViewBuilder};

pub fn build_webview(
    window: &Window,
    adblock_engine: &AdblockEngine,
    url: &str,
    tab_id: u32,
    hardware_acceleration: bool,
    ipc_tx: crossbeam_channel::Sender<crate::ipc::BrowserIpcEnvelope>,
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
            let _ = tx_nav.send(crate::ipc::BrowserIpcEnvelope::trusted_tab_event(
                tab_id,
                format!("{}|url|{}", tab_id, url),
            ));
        }
    });

    let adblock_engine_clone = adblock_engine.clone();
    builder = builder.with_navigation_handler(move |nav_url| {
        if adblock_engine_clone.should_block(&nav_url) {
            return false; // Bloqueia navegação host
        }
        true
    });

    builder = builder.with_custom_protocol("petal".into(), move |request| {
        // Em URLs do tipo petal://newtab, o host geralmente é "newtab" e o path é vazio ou "/" dependendo do parser,
        // mas vamos interceptar se contiver "newtab" em qualquer lugar.
        let uri_str = request.uri().to_string();
        if uri_str.contains("newtab") || uri_str.contains("local_cache") {
            let content = b"<!DOCTYPE html><html><head><title>Nova Aba</title><style>body{background-color:#1e1e1e;color:#d4d4d4;font-family:monospace;display:flex;flex-direction:column;align-items:center;justify-content:center;height:100vh;margin:0;}h1{font-weight:normal;}</style></head><body><h1>Petal Browser</h1><p>Digite um endere\xc3\xa7o ou pesquisa na barra de tarefas.</p></body></html>";
            wry::http::Response::builder()
                .header(wry::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(content.to_vec().into())
                .unwrap()
        } else {
            wry::http::Response::builder()
                .status(404)
                .body(b"Not Found".to_vec().into())
                .unwrap()
        }
    });

    // Injeção de IPC para rastrear Document Title (nativo não suportado cross-platform sem extensões)
    builder = builder.with_ipc_handler(move |request| {
        let _ = ipc_tx.send(crate::ipc::BrowserIpcEnvelope::content_webview(
            tab_id, request,
        ));
    });

    let blocked_array_js = adblock_engine.get_blocked_domains_js_array();
    let init_script = format!(
        r#"
        (function() {{
            if (window.__petal_init) return;
            window.__petal_init = true;

            const blocked = {};
            function isBlocked(urlStr) {{
                if (!urlStr) return false;
                try {{
                    let parsed = new URL(urlStr, window.location.href);
                    let host = parsed.hostname.toLowerCase();
                    for (let i = 0; i < blocked.length; i++) {{
                        if (host === blocked[i] || host.endsWith('.' + blocked[i])) return true;
                    }}
                }} catch(e) {{}}
                return false;
            }}

            if (window.fetch) {{
                const origFetch = window.fetch;
                window.fetch = async function(...args) {{
                    let url = (typeof args[0] === 'string') ? args[0] : (args[0] && args[0].url);
                    if (isBlocked(url)) return Promise.reject(new Error('Petal Adblock: Fetch blocked'));
                    return origFetch.apply(this, args);
                }};
            }}

            if (window.XMLHttpRequest && XMLHttpRequest.prototype.open) {{
                const origOpen = XMLHttpRequest.prototype.open;
                XMLHttpRequest.prototype.open = function(...args) {{
                    if (isBlocked(args[1])) return;
                    return origOpen.apply(this, args);
                }};
            }}

            if (window.navigator && navigator.sendBeacon) {{
                const origBeacon = navigator.sendBeacon;
                navigator.sendBeacon = function(url, data) {{
                    if (isBlocked(url)) return false;
                    return origBeacon.call(navigator, url, data);
                }};
            }}

            try {{
                new MutationObserver((mutations) => {{
                    for (let i = 0; i < mutations.length; i++) {{
                        let added = mutations[i].addedNodes;
                        for (let j = 0; j < added.length; j++) {{
                            let n = added[j];
                            if (n.nodeType === 1) {{
                                let tag = n.nodeName;
                                if ((tag === 'SCRIPT' || tag === 'IFRAME' || tag === 'IMG') && n.src) {{
                                    if (isBlocked(n.src)) {{
                                        n.src = '';
                                        n.remove();
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}).observe(document, {{ childList: true, subtree: true }});
            }} catch(e) {{}}

            window.addEventListener('keydown', function(e) {{
                if (e.ctrlKey && e.key && e.key.toLowerCase() === 'l') {{
                    e.preventDefault();
                    if (window.ipc) window.ipc.postMessage('{}|focus_omnibox|');
                }}
            }});

            let lastTitle = null;
            let titleTimeout = null;
            function notifyTitle(t) {{
                let cleanTitle = (t || '').trim();
                if (cleanTitle === lastTitle) return;
                lastTitle = cleanTitle;
                
                if (titleTimeout) clearTimeout(titleTimeout);
                titleTimeout = setTimeout(() => {{
                    if (window.ipc) window.ipc.postMessage('{}|title|' + cleanTitle);
                }}, 150);
            }}

            // Interceptação direta para SPAs que reescrevem document.title via JS
            try {{
                const NativeTitle = Object.getOwnPropertyDescriptor(Document.prototype, 'title');
                if (NativeTitle && NativeTitle.set) {{
                    Object.defineProperty(document, 'title', {{
                        get: function() {{ return NativeTitle.get.call(this); }},
                        set: function(val) {{
                            NativeTitle.set.call(this, val);
                            notifyTitle(val);
                        }}
                    }});
                }}
            }} catch(e) {{}}

            try {{
                let titleElement = null;
                let titleObs = new MutationObserver(() => notifyTitle(document.title));
                
                function observeTitleElement(el) {{
                    if (!el || titleElement === el) return;
                    titleElement = el;
                    titleObs.disconnect();
                    titleObs.observe(titleElement, {{ childList: true, characterData: true, subtree: true }});
                }}

                let headObs = new MutationObserver((mutations) => {{
                    let shouldNotify = false;
                    for (let i = 0; i < mutations.length; i++) {{
                        let added = mutations[i].addedNodes;
                        for (let j = 0; j < added.length; j++) {{
                            if (added[j].nodeName === 'TITLE') {{
                                observeTitleElement(added[j]);
                                shouldNotify = true;
                            }}
                        }}
                    }}
                    if (shouldNotify) notifyTitle(document.title);
                }});

                function initObservers() {{
                    let head = document.querySelector('head') || document.documentElement;
                    if (head) headObs.observe(head, {{ childList: true }});
                    
                    let target = document.querySelector('title');
                    if (target) observeTitleElement(target);
                    
                    notifyTitle(document.title);
                }}

                if (document.body || document.head) {{
                    initObservers();
                }} else {{
                    document.addEventListener('DOMContentLoaded', initObservers);
                }}
            }} catch(e) {{}}
        }})();
    "#,
        blocked_array_js, tab_id, tab_id
    );
    builder = builder.with_initialization_script(&init_script);

    #[cfg(target_os = "windows")]
    {
        let mut args = "--js-flags=\"--lite-mode --max-old-space-size=128 --scavenger_max_new_space_capacity_mb=4\" --renderer-process-limit=2".to_string();
        if !hardware_acceleration {
            args.push_str(" --disable-gpu");
        }
        builder = builder.with_additional_browser_args(&args);
    }
    builder.with_url(url)?.build()
}
