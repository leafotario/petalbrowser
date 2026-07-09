use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    pub search_engine: String,
    pub hardware_acceleration: bool,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            search_engine: "https://duckduckgo.com/?q={}".to_string(),
            hardware_acceleration: true,
        }
    }
}

impl BrowserConfig {
    pub fn validate(&mut self) {
        let trimmed = self.search_engine.trim();
        if trimmed.is_empty() || !trimmed.contains("{}") {
            self.search_engine = "https://duckduckgo.com/?q={}".to_string();
        } else {
            self.search_engine = trimmed.to_string();
        }
    }

    fn base_dir() -> Result<PathBuf, String> {
        let mut base = if cfg!(target_os = "windows") {
            if let Ok(appdata) = std::env::var("APPDATA") {
                PathBuf::from(appdata)
            } else {
                return Err("Variável de ambiente APPDATA não encontrada no Windows.".to_string());
            }
        } else if cfg!(target_os = "macos") {
            if let Ok(home) = std::env::var("HOME") {
                let mut p = PathBuf::from(home);
                p.push("Library");
                p.push("Application Support");
                p
            } else {
                return Err("Variável de ambiente HOME não encontrada no macOS.".to_string());
            }
        } else {
            if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
                PathBuf::from(xdg_config)
            } else if let Ok(home) = std::env::var("HOME") {
                let mut p = PathBuf::from(home);
                p.push(".config");
                p
            } else {
                return Err(
                    "Variáveis de ambiente XDG_CONFIG_HOME ou HOME não encontradas no Linux/Unix."
                        .to_string(),
                );
            }
        };
        base.push("PetalBrowser");
        if let Err(e) = fs::create_dir_all(&base) {
            return Err(format!(
                "Falha de permissão ao criar diretório de configuração ({:?}): {}",
                base, e
            ));
        }
        Ok(base)
    }

    fn json_path() -> Result<PathBuf, String> {
        let mut p = Self::base_dir()?;
        p.push("config.json");
        Ok(p)
    }

    fn ini_path() -> Result<PathBuf, String> {
        let mut p = Self::base_dir()?;
        p.push("config.ini");
        Ok(p)
    }

    fn unescape_value_legacy(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('\\') => out.push('\\'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    pub fn load() -> Self {
        let mut config = BrowserConfig::default();
        let json_path = match Self::json_path() {
            Ok(p) => p,
            Err(e) => {
                println!("⚠️ Aviso Crítico de Configuração: {}", e);
                return config;
            }
        };

        // 1. Tentar carregar JSON moderno
        if json_path.exists() {
            match fs::read_to_string(&json_path) {
                Ok(content) => match serde_json::from_str::<BrowserConfig>(&content) {
                    Ok(mut c) => {
                        c.validate();
                        return c;
                    }
                    Err(e) => {
                        println!("⚠️ Erro ao decodificar config.json: {}. Criando backup do arquivo corrompido.", e);
                        let mut corrupt_path = json_path.clone();
                        corrupt_path.set_extension("json.corrupted");
                        let _ = fs::rename(&json_path, corrupt_path);
                    }
                },
                Err(e) => {
                    println!("⚠️ Falha de I/O ao ler config.json: {}. O arquivo original não será modificado.", e);
                    return config;
                }
            }
        } else {
            // 2. Fallback de migração: Ler config.ini velho se json não existir
            if let Ok(ini_path) = Self::ini_path() {
                if ini_path.exists() {
                    if let Ok(content) = fs::read_to_string(&ini_path) {
                        for line in content.lines() {
                            let trimmed = line.trim();
                            if trimmed.is_empty() || trimmed.starts_with('#') {
                                continue;
                            }
                            if let Some((k, v)) = trimmed.split_once('=') {
                                let k = k.trim();
                                let v = v.trim();
                                match k {
                                    "search_engine" => {
                                        config.search_engine = Self::unescape_value_legacy(v)
                                    }
                                    "hardware_acceleration" => {
                                        config.hardware_acceleration = v == "true"
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    config.validate();

                    // Grava em JSON para completar a migração e renomeia o ini
                    if let Err(e) = config.save() {
                        println!(
                            "⚠️ Aviso: Falha ao migrar config.ini para config.json: {}",
                            e
                        );
                    } else {
                        let mut bak_path = ini_path.clone();
                        bak_path.set_extension("ini.bak");
                        let _ = fs::rename(&ini_path, bak_path);
                    }
                    return config;
                }
            }

            // 3. Primeira execução limpa
            if let Err(e) = config.save() {
                println!(
                    "⚠️ Aviso: Falha na persistência ao tentar criar config.json. Detalhe: {}",
                    e
                );
            }
        }
        config
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::json_path()?;
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Erro ao serializar JSON: {}", e))?;

        let mut tmp_path = path.clone();
        tmp_path.set_extension("tmp");

        if let Err(e) = fs::write(&tmp_path, &content) {
            return Err(format!(
                "Erro de disco ao escrever no temporário ({:?}): {}",
                tmp_path, e
            ));
        }

        if let Err(e) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(format!(
                "Erro ao renomear arquivo temporário para final: {}",
                e
            ));
        }

        Ok(())
    }
}
