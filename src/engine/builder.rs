use winit::window::Window;
use wry::{Rect, WebView, WebViewBuilder};
#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;
use crate::network::adblock::AdblockEngine;

pub fn build_webview(
    window: &Window,
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

    let adblock_engine_clone = adblock_engine.clone();
    builder = builder.with_navigation_handler(move |nav_url| {
        if adblock_engine_clone.should_block(&nav_url) {
            return false; // Bloqueia navegação host
        }
        true
    });

    // Injeção de IPC para rastrear Document Title (nativo não suportado cross-platform sem extensões)
    builder = builder.with_ipc_handler(move |request| {
        let msg = request; // request is a String in wry
        let _ = ipc_tx.send(msg);
    });

    let blocked_array_js = adblock_engine.get_blocked_domains_js_array();
    let init_script = format!(r#"
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
                    for (let m of mutations) {{
                        for (let n of m.addedNodes) {{
                            if (n.nodeType === 1) {{
                                if ((n.tagName === 'SCRIPT' || n.tagName === 'IFRAME' || n.tagName === 'IMG') && isBlocked(n.src)) {{
                                    n.src = '';
                                    n.remove();
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
            function notifyTitle(t) {{
                if (t !== lastTitle) {{
                    lastTitle = t;
                    if (window.ipc) window.ipc.postMessage('{}|title|' + t);
                }}
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
                let titleObs = new MutationObserver(() => notifyTitle(document.title || ''));
                let titleObserved = false;
                
                function tryObserveTitle() {{
                    if (titleObserved) return;
                    let target = document.querySelector('title');
                    if (target) {{
                        titleObs.disconnect();
                        titleObs.observe(target, {{ childList: true, characterData: true, subtree: true }});
                        titleObserved = true;
                    }}
                }}

                let headObs = new MutationObserver((mutations) => {{
                    for (let m of mutations) {{
                        for (let n of m.addedNodes) {{
                            if (n.nodeName === 'TITLE') {{
                                tryObserveTitle();
                                notifyTitle(document.title || '');
                                headObs.disconnect();
                                return;
                            }}
                        }}
                    }}
                }});

                if (document.head) {{
                    headObs.observe(document.head, {{ childList: true }});
                    tryObserveTitle();
                }} else {{
                    document.addEventListener('DOMContentLoaded', () => {{
                        if (document.head) headObs.observe(document.head, {{ childList: true }});
                        tryObserveTitle();
                        notifyTitle(document.title || '');
                    }});
                }}
                notifyTitle(document.title || '');
            }} catch(e) {{}}
        }})();
    "#, blocked_array_js, tab_id, tab_id);
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
