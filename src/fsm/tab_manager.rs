use std::time::Instant;
use wry::WebView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    Level0Focus,
    Level1Emptying,
    Level2Suspended,
    Level3Terminal,
}

pub struct Tab {
    pub id: u32,
    pub url: String,
    pub title: String,
    pub snapshot: Option<Vec<u8>>,
    pub state: TabState,
    pub background_since: Option<Instant>,
}

pub enum FsmAction {
    None,
    RequestDestruction,
}

pub struct TabManager {
    pub tabs: Vec<Tab>,
    pub active_index: usize,
    next_id: u32,
}

impl TabManager {
    pub fn new() -> Self {
        let mut tm = Self {
            tabs: Vec::new(),
            active_index: 0,
            next_id: 1,
        };
        tm.new_tab("https://magma.browser/local_cache".to_string());
        tm
    }

    pub fn new_tab(&mut self, url: String) {
        let tab = Tab {
            id: self.next_id,
            url,
            title: String::new(),
            snapshot: None,
            state: TabState::Level0Focus,
            background_since: None,
        };
        self.next_id += 1;
        self.tabs.push(tab);
        self.active_index = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self, index: usize) -> Option<u32> {
        if index < self.tabs.len() {
            let removed_id = self.tabs.remove(index).id;
            if self.tabs.is_empty() {
                self.new_tab("https://magma.browser/local_cache".to_string());
            } else if self.active_index >= self.tabs.len() {
                self.active_index = self.tabs.len() - 1;
            } else if index < self.active_index {
                self.active_index -= 1;
            }
            Some(removed_id)
        } else {
            None
        }
    }

    pub fn get_active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_index)
    }

    pub fn get_active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_index)
    }

    pub fn switch_tab(&mut self, new_index: usize) -> bool {
        if new_index < self.tabs.len() && new_index != self.active_index {
            // Marca a aba atual como inativa
            if let Some(current) = self.tabs.get_mut(self.active_index) {
                current.state = TabState::Level0Focus; // Vai sofrer decaimento depois
                current.background_since = Some(Instant::now());
            }
            
            self.active_index = new_index;
            
            // Reativa a nova aba
            if let Some(target) = self.tabs.get_mut(self.active_index) {
                target.state = TabState::Level0Focus;
                target.background_since = None;
            }
            return true;
        }
        false
    }

    pub fn set_foreground(&mut self, webview: &WebView) -> Result<(), String> {
        if let Some(target) = self.get_active_tab_mut() {
            target.state = TabState::Level0Focus;
            target.background_since = None;
        }
        #[cfg(target_os = "windows")] return resume_webview_windows(webview);
        #[cfg(not(target_os = "windows"))] return Ok(());
    }

    pub fn get_tab_mut(&mut self, id: u32) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn update_tab_title(&mut self, id: u32, title: String) {
        if let Some(tab) = self.get_tab_mut(id) {
            tab.title = title;
        }
    }

    pub fn update_tab_url(&mut self, id: u32, url: String) {
        if let Some(tab) = self.get_tab_mut(id) {
            tab.url = url;
        }
    }

    pub fn update_active_url(&mut self, url: String) {
        if let Some(active) = self.get_active_tab_mut() {
            active.url = url;
        }
    }

    pub fn save_active_snapshot(&mut self, frame_data: Vec<u8>) {
        if let Some(active) = self.get_active_tab_mut() {
            let compressed = lz4_flex::compress_prepend_size(&frame_data);
            active.snapshot = Some(compressed);
        }
    }

    pub fn tick(&mut self, _webview: &WebView) -> Result<FsmAction, String> {
        let action = FsmAction::None;
        
        // As abas inativas não possuem WebView, logo a FSM de decaimento de memória
        // foca apenas em colocar o working set/prioridade da ativa para baixo se ociosidade.
        // Como o usuário determinou QUE SÓ 1 WEBVIEW EXISTA, a aba inativa NÃO tem WebView.
        // O decaimento Nível 1/2/3 então afeta a PRÓPRIA aba ativa caso o usuário fique parado?
        // Sim, o OsTrimmer lida com a redução host, mas FSM lida com o Motor JS de abas abertas ociosas.
        
        // Aqui o TabManager gerencia o state da Aba ATIVA se ela ficar inativa
        // E como abas background não tem webview, a lógica Level 1 e 2 só afetam a aba em foco quando ociosidade do usuário ocorre.
        
        // Para alinhar com a nova diretiva "1 webview por vez", a aba INATIVA morre 100%. 
        // Portanto, a FSM de Níveis 1,2,3 para abas background perde sentido no contexto Wry (já que a Wry delas foi destruida!).
        // O que podemos fazer é: Se o Winit ficar idle (usuário minimizou), passamos a FSM na aba ativa.
        
        Ok(action)
    }
}

#[cfg(target_os = "windows")]
use wry::WebViewExtWindows;
#[cfg(target_os = "windows")]
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2Controller, ICoreWebView2_19, ICoreWebView2_3,
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
};
#[cfg(target_os = "windows")]
use windows_core::ComInterface;

#[cfg(target_os = "windows")]
fn get_core_webview2(webview: &WebView) -> Result<ICoreWebView2Controller, String> {
    Ok(webview.controller())
}

#[cfg(target_os = "windows")]
fn resume_webview_windows(webview: &WebView) -> Result<(), String> {
    unsafe {
        let controller = get_core_webview2(webview)?;
        let core = controller.CoreWebView2().map_err(|e| format!("COM falha: {}", e))?;
        if let Ok(core_3) = core.cast::<ICoreWebView2_3>() { let _ = core_3.Resume(); }
        if let Ok(core_19) = core.cast::<ICoreWebView2_19>() { let _ = core_19.SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL); }
        Ok(())
    }
}
