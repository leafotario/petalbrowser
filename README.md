# 🌸 Petal Browser

<p align="center">
  <img src="Petal_logo.png" alt="Petal Browser Logo" width="400">
</p>

Bem-vindo ao repositório do **Petal**. Se você cansou de navegadores que devoram toda a memória RAM do seu computador só por abrirem duas abas, você está no lugar certo.

## 📖 Para Seres Humanos: O que é o Petal?
Hoje em dia, abrir o Google Chrome, Edge ou Firefox significa ceder metade da potência do seu PC para o navegador. Eles ficaram pesados, cheios de telemetria, extensões embutidas e ferramentas que você nunca pediu.

A ideia do **Petal Browser** é voltar à essência: ser uma "janela" ultra-rápida, direta e purista para a internet. 

Como nós fazemos o Petal usar **até 99% menos RAM** que um navegador convencional? 
Nós cortamos o "teatro". O Petal não embute um motor gigantesco dentro de si mesmo. Ele pega carona no motor nativo de renderização que *já existe* escondido no seu sistema operacional (como o WebView2 no Windows ou WebKit no Mac), e constrói uma interface super minimalista por cima disso.

O resultado? 
- 🚀 **Leveza Absurda**: A interface é desenhada puramente por pixels direto na tela, sem bibliotecas pesadas.
- 🛡️ **Adblock Nativo e Real**: Não usamos extensões que pesam a máquina. O Petal injeta um bloqueador de anúncios invisível direto no fluxo da rede.
- ♻️ **Expurgo de Memória**: O navegador literalmente obriga o sistema operacional a limpar o lixo deixado para trás pelas páginas quando você está ocioso.

---

## 💻 Para Programadores: Como a Mágica Acontece?
O Petal é escrito em **Rust** 🦀. Ele nasceu como um protesto contra os *bloatwares* modernos e segue uma arquitetura *Bare-Metal*.

Se você for vasculhar o código, não vai achar React, Vue, Tauri ou Electron. O footprint é sagrado.

### Stack Tecnológica
- **Linguagem:** Rust (Edition 2021)
- **Janelas & Eventos:** `winit` (0.29) com manipulação cirúrgica de eventos Win32/X11 por baixo dos panos para garantir Handoff de Focus perfeito e suporte a IME.
- **Renderização da UI (Tab Bar / Omnibox):** `softbuffer`. Nós desenhamos a interface superior modificando os buffers de pixels manualmente. Sem shaders complexos, sem aceleração GPU overhead para UI, apenas blitting direto na CPU para que a engine não concorra com a renderização da página.
- **Motor Web (O Core):** `wry` (0.35). Mantemos um `HashMap` de WebViews vivas que rodam em processos isolados do SO.

### Por que o Petal é tecnicamente diferente?
1. **O `OsTrimmer` (O Carrasco de RAM):** No loop de eventos (`main.rs`), possuímos um monitor chamado `OsTrimmer`. No Windows, ele usa APIs COM/Win32 (`EmptyWorkingSet`, `GetProcessMemoryInfo`) para invadir o processo da WebView2 nativa e obrigar o Edge/V8 a descarregar caches e despachar o Working Set quando ocioso. Se a aba passar do teto estipulado de MegaBytes, a aba sofre *Emergency Crash* e é reciclada instantaneamente.
2. **True IPC Adblocker:** Sem dependências cegas. Um motor híbrido intercepta URLs nativamente para domínios de trackers, enquanto um Script JS é injetado (*Pre-load*) na página sobrescrevendo `window.fetch` e `XMLHttpRequest.open` para garantir bloqueio de recursos na raiz.
3. **Gerenciamento Honesto de Abas:** A State Machine de tabs (`TabManager`) atua apenas como fonte da verdade estrita. O lifecycle gráfico é ditado pelo `winit`, reduzindo o _dead code_ a **zero**.

## 🛠️ Como rodar o projeto localmente
Você precisará do [Rust e Cargo instalados](https://rustup.rs/).

1. Clone o repositório:
```bash
git clone https://github.com/leafotario/petalbrowser.git
cd petalbrowser
```

2. Rode em modo de desenvolvimento (ou Release para performance total):
```bash
cargo run --release
```

> **Nota para devs Windows**: O Petal faz uso pesado da API do Windows nativa para controlar o lifecycle das abas e renderização (ex: `FindWindowExW` para posicionamento de Hwnds filhos sob a UI do Softbuffer). Certifique-se de estar com as bibliotecas do WebView2 atualizadas no SO.

---
*Petal: Navegação purista. Menos ruído. Mais internet.*
