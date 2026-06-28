# petal browser

<p align="center">
  <img src="Petal_logo.png" alt="Petal Browser Logo" width="400">
</p>

petal browser é um navegador feito para ser leve, direto e sem firula.

a ideia é simples: abrir a internet sem transformar o computador numa carroça. a interface é própria, o visual é minimalista, e o projeto tenta gastar só o necessário com a parte da janela e dos controles, enquanto a página continua sendo renderizada pelo webview nativo do sistema.

## o que ele faz

- abre páginas da web com uma interface bem enxuta
- tem abas
- tem barra de endereço / busca
- inclui botões básicos de navegação
- traz um adblock próprio
- salva configurações localmente
- tem uma janela de configurações separada
- tenta manter o consumo de memória sob controle quando o sistema permite

## por que ele existe

porque nem todo navegador precisa parecer uma central de comando.

petal nasceu da vontade de fazer algo:
- mais leve
- mais simples de entender
- mais fácil de mexer
- menos cheio de enfeite que ninguém pediu

não é um navegador “com tudo dentro”. é um navegador que tenta ficar fora do caminho.

## para quem quer só usar

se você só quer testar o projeto:

```bash
git clone https://github.com/leafotario/petalbrowser.git
cd petalbrowser
cargo run --release
