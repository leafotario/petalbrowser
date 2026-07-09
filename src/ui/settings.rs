use crate::config::BrowserConfig;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn get_settings_html(config: &BrowserConfig) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Configurações do Petal</title>
    <style>
        body {{
            background-color: #1e1e1e;
            color: #d4d4d4;
            font-family: monospace;
            padding: 20px;
        }}
        input[type="text"] {{
            width: 100%;
            background: #2d2d2d;
            border: 1px solid #454545;
            color: #fff;
            padding: 5px;
            margin-top: 5px;
            margin-bottom: 15px;
        }}
        button {{
            background: #007acc;
            color: white;
            border: none;
            padding: 8px 16px;
            cursor: pointer;
            margin-top: 20px;
        }}
        button:hover {{
            background: #005f9e;
        }}
        .setting {{
            margin-bottom: 20px;
        }}
    </style>
</head>
<body>
    <h2>Configurações</h2>
    
    <div class="setting">
        <label>
            <input type="checkbox" id="hw_accel" {hw_checked}>
            Aceleração de Hardware (Requer reinício do app)
        </label>
    </div>

    <div class="setting">
        <label>Motor de Busca Padrão</label>
        <input type="text" id="search_engine" value="{engine}">
        <small style="display:block;margin-top:5px;color:#888;">Use {{}} onde a pesquisa deve ser inserida.</small>
    </div>

    <div style="margin-top: 15px; margin-bottom: 20px; font-size: 0.85em; color: #888;">
        <i>ℹ️ As configurações são persistidas em seu Perfil Local (AppData, XDG, ou Home). O diretório de execução atual não afeta o salvamento.</i>
    </div>

    <div id="error_msg" style="color: #ff5555; display: none; margin-top: 10px; margin-bottom: 15px; font-weight: bold;"></div>

    <button onclick="save()">Salvar e Fechar</button>

    <script>
        function save() {{
            var hw = document.getElementById('hw_accel').checked;
            var engine = document.getElementById('search_engine').value.trim();
            
            if (engine !== '' && engine.indexOf('{{}}') === -1) {{
                var err = document.getElementById('error_msg');
                err.innerText = 'Erro: O motor de busca deve conter "{{}}" para o termo pesquisado.';
                err.style.display = 'block';
                return;
            }}
            
            var payload = 'save_config:' + JSON.stringify({{ hardware_acceleration: hw, search_engine: engine }});
            window.ipc.postMessage(payload);
        }}
    </script>
</body>
</html>"#,
        hw_checked = if config.hardware_acceleration {
            "checked"
        } else {
            ""
        },
        engine = escape_html(&config.search_engine)
    )
}
