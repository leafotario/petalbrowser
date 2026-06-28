
#[derive(Clone)]
pub struct AdblockEngine {
    rules: Vec<(String, String)>,
}

impl AdblockEngine {
    pub fn start() -> Self {
        // Inicializa com uma lista de domínios conhecidos de anúncios e tracking.
        // NOTA DE ARQUITETURA: O motor é estritamente Host-Only (DNS-level like).
        // Regras contendo caminhos (paths) como "facebook.com/tr/" são mortas e 
        // nunca funcionarão porque o avaliador extrai apenas o host. 
        // Para manter o footprint de RAM mínimo, mantemos a simplicidade de Host-Only.
        let mut rules = Vec::new();
        let base_list = vec![
            "doubleclick.net",
            "google-analytics.com",
            "googlesyndication.com",
            "adservice.google.com",
            "amazon-adsystem.com",
            "taboola.com",
            "outbrain.com",
            "criteo.com",
            "adsafeprotected.com",
            "adnxs.com",
            "adform.net",
            "connect.facebook.net",
            "pixel.facebook.com",
            "hotjar.com",
            "clarity.ms",
        ];
        
        for d in base_list {
            // Pré-computa o sufixo (".dominio") para evitar alocações no caminho quente
            rules.push((d.to_string(), format!(".{}", d)));
        }

        Self {
            rules,
        }
    }

    fn extract_host(url: &str) -> Option<&str> {
        let after_scheme = url.split("://").nth(1).unwrap_or(url);
        let host_port = after_scheme.split('/').next().unwrap_or(after_scheme);
        let host = host_port.split(':').next().unwrap_or(host_port);
        if host.is_empty() { None } else { Some(host) }
    }

    /// Analisa se a URL dada (navegação) pertence a algum domínio bloqueado.
    /// Como o Petal foca em extrema eficiência de RAM, este bloqueio é estritamente
    /// comparado na raiz do host, ignorando paths e query params.
    pub fn should_block(&self, url: &str) -> bool {
        // Normaliza a URL
        let normalized = url.to_lowercase();
        
        // Exceções para esquemas nativos/locais globais
        if normalized.starts_with("petal://") || normalized.starts_with("file://") {
            return false;
        }

        if let Some(host) = Self::extract_host(&normalized) {
            // Bypass seguro para servidores locais
            if host == "localhost" || host.starts_with("127.0.0.1") {
                return false;
            }

            for (domain, suffix) in &self.rules {
                if host == domain || host.ends_with(suffix) {
                    // Log útil e sutil apenas quando bloqueia de fato
                    println!("🛡️ Adblock interceptou navegação para: {}", domain);
                    return true;
                }
            }
        }
        false
    }

    /// Retorna a lista de domínios bloqueados como uma string de Array JSON
    /// Útil para injetar o escudo dinâmico no JS, que também segue a regra Host-Only.
    pub fn get_blocked_domains_js_array(&self) -> String {
        let items: Vec<String> = self.rules.iter().map(|(d, _)| format!("'{}'", d)).collect();
        format!("[{}]", items.join(","))
    }
}
