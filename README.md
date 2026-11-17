# Google Keep Desktop

Aplicativo desktop leve e nativo para o **Google Keep** no Windows, construído com **Rust**, **Tauri v2** e **WebView2** (`wry`).

---

## Por que este projeto foi feito?

O Google Keep não possui um aplicativo desktop oficial para Windows — a única opção padrão é mantê-lo aberto em uma aba do navegador ou como atalho simples de navegador. Isso traz alguns incômodos no dia a dia:

- **Fica perdido entre dezenas de abas** e consome memória de uma instância extra do navegador.
- **Não integra com o sistema operacional**: não há ícone na bandeja do sistema (*System Tray*), atalho global para anotar de qualquer tela, nem opção de *Sempre no topo* (*Always on Top*).
- **Fluxo lento para anotações rápidas**: abrir o navegador, achar a aba certa e clicar no campo de nota quebra o raciocínio durante o trabalho.

O objetivo do **Google Keep Desktop** é oferecer uma janela dedicada, discreta, com inicialização rápida usando o próprio motor **WebView2** já instalado no Windows (sem empacotar um Chromium inteiro como no Electron) e recursos pensados para uso contínuo em desktop.

---

## Recursos atuais (`v0.2.0`)

- **Bandeja do sistema (System Tray)**:
  - Clique esquerdo no ícone alterna entre mostrar e ocultar a janela.
  - Fechar no `X` apenas oculta o app na bandeja, mantendo sua sessão do Google ativa e abertura instantânea.
- **Atalhos globais (funcionam em qualquer tela do Windows)**:
  - `Ctrl + Alt + K` — Abre ou oculta a janela do Google Keep.
  - `Ctrl + Alt + N` — Mostra a janela e foca direto no campo **"Criar uma nota..."** para anotação rápida.
- **Iniciar com o Windows**:
  - Alternável pelo menu da bandeja (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), iniciando minimizado na bandeja (`--minimized`).
- **Sempre no topo (*Always on Top*)**:
  - Mantém o Keep flutuando sobre outros aplicativos quando necessário.
- **Persistência automática (`desktop-config.json`)**:
  - Lembra tamanho, posição (validada nos monitores conectados), estado de *Sempre no topo*, autostart e nível de zoom.
- **Controle de zoom persistente**:
  - Submenu **Zoom** na bandeja (`+10%`, `-10%`, `Restaurar 100%`), reaplicado automaticamente ao carregar.
- **Links externos no navegador padrão**:
  - Links clicados dentro das notas (inclusive redirecionamentos `google.com/url?q=...`) abrem no seu navegador padrão do Windows via `ShellExecuteW`, mantendo dentro do app apenas o Google Keep e telas de autenticação Google.
- **Barra de título escura nativa**:
  - Tema escuro na moldura da janela e no WebView2 para combinar com o modo escuro do Keep.
- **DevTools integrado**:
  - Acesso direto pelo menu da bandeja (*Abrir DevTools (F12)*) para inspeção e depuração.

---

## Como foi feito (Arquitetura e Soluções Técnicas)

### Stack utilizada

- **Rust + Tauri v2 (`tauri`)**: gerencia ciclo de vida da aplicação, janela nativa, bandeja do sistema (*System Tray*), atalhos globais (`tauri-plugin-global-shortcut`) e geração do instalador NSIS para Windows.
- **WebView pura com `wry` (`WebView2`)**: renderiza diretamente `https://keep.google.com` usando o Edge WebView2 nativo do Windows.
- **APIs Win32 (`windows`, `webview2-com`, `winreg`)**: controle fino de perfil/privacidade do WebView2, abertura de URLs externas com `ShellExecuteW` e gerenciamento do registro de inicialização automática no Windows.

### O principal desafio técnico (e por que a arquitetura é assim)

Ao tentar carregar o Google Keep dentro de uma `WebviewWindow` padrão do Tauri, ocorreram duas falhas críticas que impediam o carregamento das notas após o login:

1. **Colisão com `window.ipc` no bundle minificado do Google Keep**:
   - O runtime padrão do Tauri injeta `Object.defineProperty(window, 'ipc', { value: Object.freeze(...) })` antes de a página executar.
   - O JavaScript minificado pelo *Closure Compiler* do Google Keep (`m=n`) declara uma variável minificada com o nome `ipc` e tenta definir propriedades nela (`C(ipc, Base)`), disparando `TypeError: Cannot set properties of undefined (setting 'constructor')` e abortando toda a inicialização (`initNotesApp`, `__gapiOnLoad`).
2. **Travamento de Service Worker no WebView2 (`WebView2Feedback#5764`)**:
   - Os filtros `WebResourceRequested` registrados por protocolos customizados (`tauri://`, `ipc://`) faziam o Service Worker do Keep com `InstallEvent.addRoutes` travar no estado `STARTING`, deixando o corpo da página em branco.

**Como resolvemos em [`src-tauri/src/main.rs`](src-tauri/src/main.rs):**
- Em vez de declarar janelas em `tauri.conf.json`, a janela nativa é criada diretamente com `tauri::Window::builder` (feature `unstable` do Tauri v2) sem `WebviewWindow`.
- Sobre essa janela, anexamos uma instância pura de `wry::WebViewBuilder` (sem `ipc_handler` e sem `custom_protocol`), armazenada em `thread_local!` na thread principal.
- Antes de navegar para `https://keep.google.com`, configuramos o perfil do WebView2 no Windows com `COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE`, passamos flags de compatibilidade (`--disable-features=...ServiceWorkerStaticRouter,ThirdPartyStoragePartitioning,TrackingProtection3pcd`) e limpamos Service Workers antigos sem apagar cookies/autenticação.
- Scripts pontuais (como a abertura da nota rápida em `Ctrl + Alt + N`) são executados sob demanda via `evaluate_script` dentro de IIFEs isoladas `(function(){ ... })()`, sem criar propriedades globais em `window`.

---

## Estrutura do repositório

```text
├── .github/workflows/
│   └── build-windows.yml   # CI/CD no GitHub Actions (compila no Windows e publica Release)
├── src/
│   └── index.html          # Fallback estático mínimo exigido pelo frontendDist do Tauri
├── src-tauri/
│   ├── Cargo.toml          # Dependências Rust (tauri v2, wry, webview2-com, winreg, windows)
│   ├── tauri.conf.json     # Metadados do app, ícone e configuração do instalador NSIS
│   ├── capabilities/       # Permissões padrão do Tauri
│   └── src/main.rs         # Código principal: janela, WebView2, tray, atalhos, config e links
└── README.md
```

---

## Instalação (Windows)

Baixe a versão mais recente na página de **[Releases](https://github.com/Kyouz2148/Keep-Desktop/releases)**:

- **Instalador completo**: `Google.Keep_<versao>_x64-setup.exe`
- **Executável avulso**: `keep-desktop.exe`

*(Requer o Microsoft Edge WebView2 Runtime, já pré-instalado no Windows 10/11.)*

---

## Compilação local e CI/CD

### Localmente no Windows

```powershell
npm install -g @tauri-apps/cli@^2
cargo tauri dev    # Executar em modo desenvolvimento
cargo tauri build  # Gerar binário release e instalador NSIS
```

### Automação via GitHub Actions

Todo `push` na branch `main` (ou tag `v*`) executa o workflow [`.github/workflows/build-windows.yml`](.github/workflows/build-windows.yml) em `windows-latest`, compila o `keep-desktop.exe` e o instalador NSIS e cria/atualiza automaticamente a Release correspondente à versão definida em [`src-tauri/tauri.conf.json`](src-tauri/tauri.conf.json).
