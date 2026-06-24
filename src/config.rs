use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct BrowserConfig {
    pub search_engine: String,
    pub hardware_acceleration: bool,
}

impl BrowserConfig {
    pub fn default() -> Self {
        Self {
            search_engine: "https://duckduckgo.com/?q={}".to_string(),
            hardware_acceleration: true,
        }
    }

    fn path() -> PathBuf {
        let mut base = if cfg!(target_os = "windows") {
            if let Ok(appdata) = std::env::var("APPDATA") {
                PathBuf::from(appdata)
            } else {
                std::env::current_dir().unwrap_or_default()
            }
        } else if cfg!(target_os = "macos") {
            if let Ok(home) = std::env::var("HOME") {
                let mut p = PathBuf::from(home);
                p.push("Library");
                p.push("Application Support");
                p
            } else {
                std::env::current_dir().unwrap_or_default()
            }
        } else {
            if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
                PathBuf::from(xdg_config)
            } else if let Ok(home) = std::env::var("HOME") {
                let mut p = PathBuf::from(home);
                p.push(".config");
                p
            } else {
                std::env::current_dir().unwrap_or_default()
            }
        };
        base.push("MagmaBrowser");
        let _ = fs::create_dir_all(&base);
        base.push("config.ini");
        base
    }

    pub fn load() -> Self {
        let mut config = Self::default();
        if let Ok(content) = fs::read_to_string(Self::path()) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
                if let Some((k, v)) = trimmed.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    match k {
                        "search_engine" => config.search_engine = v.to_string(),
                        "hardware_acceleration" => config.hardware_acceleration = v == "true",
                        _ => {}
                    }
                }
            }
        } else {
            let _ = config.save();
        }
        config
    }

    pub fn save(&self) -> Result<(), String> {
        let content = format!(
            "search_engine={}\nhardware_acceleration={}\n",
            self.search_engine, self.hardware_acceleration
        );
        fs::write(Self::path(), content).map_err(|e| format!("Erro de I/O ao salvar config: {}", e))
    }
}
