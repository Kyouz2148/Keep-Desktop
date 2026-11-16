# Google Keep Desktop

Aplicativo desktop leve para o Google Keep construído com **Tauri v2** para Windows.

## Recursos

- **System Tray**: clique esquerdo alterna mostrar/ocultar; menu de contexto com *Abrir Google Keep*, *Sempre no topo* e *Sair*.
- **Atalho global (`Ctrl + Alt + K`)**: abre ou oculta a janela de qualquer lugar do Windows.
- **Ocultar ao fechar**: clicar no `X` mantém o aplicativo na bandeja do sistema com a sessão autenticada.

## Compilação local (Windows)

```powershell
cargo install tauri-cli --version "^2.0.0"
cargo tauri dev
cargo tauri build
```
