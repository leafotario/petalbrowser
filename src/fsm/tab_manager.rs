pub struct Tab {
    pub id: u32,
    pub url: String,
    pub title: String,
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
            self.active_index = new_index;
            return true;
        }
        false
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
}
