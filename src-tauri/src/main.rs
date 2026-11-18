#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use url::Url;
use wry::PageLoadEvent;

const KEEP_URL: &str = "https://keep.google.com";
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 3.0;
const ZOOM_STEP: f64 = 0.1;

static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
const AUTOSTART_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const AUTOSTART_VALUE_NAME: &str = "GoogleKeepDesktop";
#[cfg(target_os = "windows")]
const AUTOSTART_MENU_LABEL: &str = "Iniciar com o Windows";
#[cfg(not(target_os = "windows"))]
const AUTOSTART_MENU_LABEL: &str = "Iniciar com o sistema";

const QUICK_NOTE_HTML: &str = r#"<!DOCTYPE html>
<html lang="pt-BR">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Nova nota rápida</title>
<style>
  :root {
    color-scheme: dark;
  }
  * {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
  }
  html, body {
    width: 100%;
    height: 100%;
    background: #202124;
    color: #e8eaed;
    font-family: 'Segoe UI', system-ui, -apple-system, Roboto, sans-serif;
    overflow: hidden;
    user-select: none;
  }
  .card {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #202124;
    border: 1px solid #3c4043;
  }
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid #2d2e30;
    background: #252629;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    color: #e8eaed;
  }
  .brand-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #fbbc04;
    box-shadow: 0 0 8px rgba(251, 188, 4, 0.45);
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .link-btn {
    border: 1px solid #3c4043;
    background: transparent;
    color: #9aa0a6;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 5px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .link-btn:hover {
    background: #303134;
    color: #e8eaed;
  }
  .close-btn {
    width: 24px;
    height: 24px;
    border: none;
    background: transparent;
    color: #9aa0a6;
    font-size: 18px;
    line-height: 1;
    border-radius: 5px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .close-btn:hover {
    background: #35363a;
    color: #e8eaed;
  }
  .fields {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 12px 16px 8px;
    gap: 8px;
    min-height: 0;
  }
  #note-title {
    width: 100%;
    border: none;
    outline: none;
    background: transparent;
    color: #e8eaed;
    font-size: 16px;
    font-weight: 600;
    font-family: inherit;
    padding: 4px 0;
  }
  #note-title::placeholder {
    color: #80868b;
  }
  #note-body {
    flex: 1;
    width: 100%;
    border: none;
    outline: none;
    resize: none;
    background: transparent;
    color: #e8eaed;
    font-size: 14px;
    line-height: 1.5;
    font-family: inherit;
    padding: 4px 0;
  }
  #note-body::placeholder {
    color: #80868b;
  }
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-top: 1px solid #2d2e30;
    background: #252629;
  }
  .hint {
    font-size: 11px;
    color: #9aa0a6;
  }
  .hint.warn {
    color: #fbbc04;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .btn-ghost {
    border: 1px solid #3c4043;
    background: transparent;
    color: #e8eaed;
    font-size: 12px;
    font-weight: 500;
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn-ghost:hover {
    background: #303134;
  }
  .btn-primary {
    border: none;
    background: #fbbc04;
    color: #202124;
    font-size: 12px;
    font-weight: 600;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn-primary:hover {
    background: #fcc934;
  }
</style>
</head>
<body>
  <div class="card">
    <div class="header">
      <div class="brand">
        <span class="brand-dot"></span>
        <span>Nova nota rápida</span>
      </div>
      <div class="header-actions">
        <button id="btn-open-main" class="link-btn" type="button" title="Abrir janela completa do Keep">Abrir Keep</button>
        <button id="btn-close" class="close-btn" type="button" title="Fechar (Esc)">&times;</button>
      </div>
    </div>
    <div class="fields">
      <input id="note-title" type="text" placeholder="Título" autocomplete="off" />
      <textarea id="note-body" placeholder="Criar uma nota..."></textarea>
    </div>
    <div class="footer">
      <span id="hint-text" class="hint">Ctrl+Enter salvar &middot; Esc fechar</span>
      <div class="actions">
        <button id="btn-cancel" class="btn-ghost" type="button">Cancelar</button>
        <button id="btn-save" class="btn-primary" type="button">Salvar nota</button>
      </div>
    </div>
  </div>
  <script>
    (function () {
      var DEFAULT_HINT = 'Ctrl+Enter salvar \u00b7 Esc fechar';
      var titleEl = document.getElementById('note-title');
      var bodyEl = document.getElementById('note-body');
      var hintEl = document.getElementById('hint-text');

      function send(payload) {
        if (window.ipc && typeof window.ipc.postMessage === 'function') {
          window.ipc.postMessage(JSON.stringify(payload));
        }
      }

      window.focusQuickNote = function () {
        hintEl.textContent = DEFAULT_HINT;
        hintEl.classList.remove('warn');
        setTimeout(function () {
          bodyEl.focus();
        }, 20);
      };

      window.resetQuickNote = function () {
        titleEl.value = '';
        bodyEl.value = '';
        hintEl.textContent = DEFAULT_HINT;
        hintEl.classList.remove('warn');
      };

      function submitNote() {
        var title = titleEl.value.trim();
        var body = bodyEl.value.trim();
        if (!title && !body) {
          hintEl.textContent = 'Digite algo antes de salvar';
          hintEl.classList.add('warn');
          bodyEl.focus();
          return;
        }
        send({
          action: 'submit',
          title: titleEl.value,
          body: bodyEl.value
        });
      }

      function closeModal() {
        send({ action: 'close' });
      }

      titleEl.addEventListener('keydown', function (e) {
        if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
          e.preventDefault();
          bodyEl.focus();
        }
      });

      document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape') {
          e.preventDefault();
          closeModal();
        } else if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
          e.preventDefault();
          submitNote();
        }
      });

      document.getElementById('btn-save').addEventListener('click', submitNote);
      document.getElementById('btn-cancel').addEventListener('click', closeModal);
      document.getElementById('btn-close').addEventListener('click', closeModal);
      document.getElementById('btn-open-main').addEventListener('click', function () {
        send({ action: 'open_main' });
      });
      document.querySelector('.header').addEventListener('mousedown', function (e) {
        if (e.button === 0 && !e.target.closest('button')) {
          send({ action: 'drag' });
        }
      });

      window.addEventListener('DOMContentLoaded', function () {
        bodyEl.focus();
      });
    })();
  </script>
</body>
</html>"#;

fn build_submit_note_js(title: &str, body: &str) -> String {
    let title_json = serde_json::to_string(title).unwrap_or_else(|_| "\"\"".to_string());
    let body_json = serde_json::to_string(body).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"(function (titleText, bodyText) {{
  var attempts = 0;
  var maxAttempts = 50;

  function isNotHiddenByStyle(el) {{
    if (!el) return false;
    var cur = el;
    while (cur && cur.nodeType === 1) {{
      var st = window.getComputedStyle(cur);
      if (st.display === 'none' || st.visibility === 'hidden') return false;
      cur = cur.parentElement;
    }}
    return true;
  }}

  function findCollapsedOrBodyComposer() {{
    var selectors = [
      'div[role="textbox"][aria-label*="Criar uma nota" i]',
      'div[role="textbox"][aria-label*="Take a note" i]',
      '[aria-label="Criar uma nota..."]',
      '[aria-label="Take a note..."]'
    ];
    for (var s = 0; s < selectors.length; s++) {{
      var list = document.querySelectorAll(selectors[s]);
      for (var i = 0; i < list.length; i++) {{
        if (!list[i].closest('[role="dialog"]') && isNotHiddenByStyle(list[i])) {{
          return list[i];
        }}
      }}
    }}
    var boxes = document.querySelectorAll('div[role="textbox"][contenteditable="true"]');
    for (var j = 0; j < boxes.length; j++) {{
      if (!boxes[j].closest('[role="dialog"]') && isNotHiddenByStyle(boxes[j])) {{
        return boxes[j];
      }}
    }}
    return null;
  }}

  function findTitleBox() {{
    var list = document.querySelectorAll(
      'div[role="textbox"][aria-label*="Título" i], div[role="textbox"][aria-label*="Title" i]'
    );
    for (var i = 0; i < list.length; i++) {{
      if (!list[i].closest('[role="dialog"]') && isNotHiddenByStyle(list[i])) {{
        return list[i];
      }}
    }}
    return null;
  }}

  function findBodyBox(titleBox) {{
    var list = document.querySelectorAll(
      'div[role="textbox"][aria-label*="Criar uma nota" i], div[role="textbox"][aria-label*="Take a note" i]'
    );
    for (var i = 0; i < list.length; i++) {{
      if (!list[i].closest('[role="dialog"]') && isNotHiddenByStyle(list[i])) {{
        return list[i];
      }}
    }}
    if (titleBox) {{
      var root = titleBox.closest('div[tabindex]') || (titleBox.parentElement && titleBox.parentElement.parentElement && titleBox.parentElement.parentElement.parentElement);
      if (root) {{
        var boxes = root.querySelectorAll('div[role="textbox"][contenteditable="true"]');
        for (var j = 0; j < boxes.length; j++) {{
          if (boxes[j] !== titleBox && isNotHiddenByStyle(boxes[j])) {{
            return boxes[j];
          }}
        }}
      }}
    }}
    return null;
  }}

  function findCloseButton(referenceEl) {{
    var scopes = [];
    if (referenceEl) {{
      var p = referenceEl.parentElement;
      for (var d = 0; d < 6 && p; d++) {{
        scopes.push(p);
        p = p.parentElement;
      }}
    }}
    scopes.push(document);
    for (var s = 0; s < scopes.length; s++) {{
      var btns = scopes[s].querySelectorAll('[role="button"], button');
      for (var i = 0; i < btns.length; i++) {{
        if (btns[i].closest('[role="dialog"]')) continue;
        var txt = (btns[i].textContent || '').trim().toLowerCase();
        if (txt === 'fechar' || txt === 'close' || txt === 'cerrar' || txt === 'fermer') {{
          if (isNotHiddenByStyle(btns[i])) return btns[i];
        }}
      }}
    }}
    return null;
  }}

  function insertIntoEditable(el, value) {{
    if (!el || !value) return;
    el.focus();
    try {{
      var sel = window.getSelection();
      var range = document.createRange();
      range.selectNodeContents(el);
      sel.removeAllRanges();
      sel.addRange(range);
      if (document.execCommand('insertText', false, value) && (el.textContent || '').length > 0) {{
        return;
      }}
    }} catch (e) {{}}

    el.textContent = value;
    try {{
      el.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: value }}));
    }} catch (e) {{
      el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    }}
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}

  function step() {{
    attempts++;
    if (attempts > maxAttempts) return;

    var openDialog = document.querySelector('[role="dialog"]');
    if (openDialog) {{
      document.dispatchEvent(
        new KeyboardEvent('keydown', {{
          key: 'Escape',
          code: 'Escape',
          keyCode: 27,
          which: 27,
          bubbles: true
        }})
      );
      setTimeout(step, 200);
      return;
    }}

    if (window.location.hash && window.location.hash !== '#home') {{
      window.location.hash = '#home';
      setTimeout(step, 250);
      return;
    }}

    var box = findCollapsedOrBodyComposer();
    if (!box) {{
      setTimeout(step, 200);
      return;
    }}

    box.dispatchEvent(new MouseEvent('mousedown', {{ bubbles: true }}));
    box.click();
    box.focus();

    setTimeout(function () {{
      var titleBox = findTitleBox();
      var bodyBox = findBodyBox(titleBox) || findCollapsedOrBodyComposer();

      if (!bodyBox && !titleBox) {{
        setTimeout(step, 200);
        return;
      }}

      var cleanTitle = (titleText || '').trim();
      var cleanBody = (bodyText || '').trimEnd();

      if (cleanTitle && titleBox) {{
        insertIntoEditable(titleBox, cleanTitle);
      }}
      if (cleanBody && bodyBox) {{
        insertIntoEditable(bodyBox, cleanBody);
      }} else if (cleanTitle && !titleBox && bodyBox) {{
        insertIntoEditable(bodyBox, cleanTitle);
      }}

      setTimeout(function () {{
        var refEl = bodyBox || titleBox;
        var closeBtn = findCloseButton(refEl);
        if (closeBtn) {{
          closeBtn.click();
        }} else if (refEl) {{
          refEl.blur();
          document.body.click();
        }}
      }}, 200);
    }}, 160);
  }}

  step();
}})({title_json}, {body_json});"#
    )
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum QuickNoteIpc {
    Submit { title: String, body: String },
    Close,
    OpenMain,
    Drag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AppConfig {
    width: Option<u32>,
    height: Option<u32>,
    x: Option<i32>,
    y: Option<i32>,
    always_on_top: bool,
    zoom: f64,
    autostart: bool,
    sw_migrated_v2: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            x: None,
            y: None,
            always_on_top: false,
            zoom: 1.0,
            autostart: false,
            sw_migrated_v2: false,
        }
    }
}

struct AppState {
    config: Mutex<AppConfig>,
    config_path: PathBuf,
}

thread_local! {
    static KEEP_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    static KEEP_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
    static QUICK_NOTE_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    static QUICK_NOTE_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
}

fn quick_note_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL), Code::KeyK)
}

fn clamp_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return 1.0;
    }
    (zoom.clamp(MIN_ZOOM, MAX_ZOOM) * 100.0).round() / 100.0
}

fn format_zoom_label(zoom: f64) -> String {
    format!("Zoom atual: {}%", (zoom * 100.0).round() as i32)
}

fn load_config(path: &Path) -> AppConfig {
    let mut cfg = fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<AppConfig>(&raw).ok())
        .unwrap_or_default();
    cfg.zoom = clamp_zoom(cfg.zoom);
    cfg
}

fn save_config(path: &Path, cfg: &AppConfig) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(path, json);
    }
}

fn save_state_config(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(cfg) = state.config.lock() {
        save_config(&state.config_path, &cfg);
    };
}

fn with_keep_webview<F>(window: &Window, f: F)
where
    F: FnOnce(&wry::WebView) + Send + 'static,
{
    let _ = window.run_on_main_thread(move || {
        KEEP_WEBVIEW.with(|cell| {
            if let Some(wv) = cell.borrow().as_ref() {
                f(wv);
            }
        });
    });
}

fn with_quick_note_webview<F>(window: &Window, f: F)
where
    F: FnOnce(&wry::WebView) + Send + 'static,
{
    let _ = window.run_on_main_thread(move || {
        QUICK_NOTE_WEBVIEW.with(|cell| {
            if let Some(wv) = cell.borrow().as_ref() {
                f(wv);
            }
        });
    });
}

fn show_and_focus_window(window: &Window) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    with_keep_webview(window, |wv| {
        let _ = wv.focus();
    });
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_visible && !is_minimized {
            save_state_config(app);
            let _ = window.hide();
        } else {
            show_and_focus_window(&window);
        }
    }
}

fn hide_quick_note(app: &AppHandle, clear: bool) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let _ = qn_win.hide();
        if clear {
            with_quick_note_webview(&qn_win, |wv| {
                let _ = wv.evaluate_script("window.resetQuickNote && window.resetQuickNote();");
            });
        }
    }
}

fn trigger_quick_note(app: &AppHandle) {
    if let Some(qn_win) = app.get_window("quick-note") {
        let is_visible = qn_win.is_visible().unwrap_or(false);
        let is_focused = qn_win.is_focused().unwrap_or(false);
        if is_visible && is_focused {
            hide_quick_note(app, true);
            return;
        }
        let _ = qn_win.center();
        let _ = qn_win.show();
        let _ = qn_win.unminimize();
        let _ = qn_win.set_focus();
        with_quick_note_webview(&qn_win, |wv| {
            let _ = wv.focus();
            let _ = wv.evaluate_script("window.focusQuickNote && window.focusQuickNote();");
        });
    }
}

fn submit_quick_note_to_keep(app: &AppHandle, title: String, body: String) {
    if title.trim().is_empty() && body.trim().is_empty() {
        return;
    }
    hide_quick_note(app, true);
    if let Some(main_win) = app.get_window("main") {
        let main_win_clone = main_win.clone();
        with_keep_webview(&main_win, move |wv| {
            if let Ok(url) = wv.url() {
                if url.contains("accounts.google.com") {
                    show_and_focus_window(&main_win_clone);
                    return;
                }
            }
            let script = build_submit_note_js(&title, &body);
            let _ = wv.evaluate_script(&script);
        });
    }
}

fn apply_zoom(app: &AppHandle, zoom_status_item: &MenuItem<tauri::Wry>, next_zoom: f64) {
    let clamped = clamp_zoom(next_zoom);
    let state = app.state::<AppState>();
    if let Ok(mut cfg) = state.config.lock() {
        cfg.zoom = clamped;
        save_config(&state.config_path, &cfg);
    }
    let _ = zoom_status_item.set_text(format_zoom_label(clamped));
    if let Some(win) = app.get_window("main") {
        with_keep_webview(&win, move |wv| {
            let _ = wv.zoom(clamped);
        });
    }
}

fn is_position_on_any_monitor(window: &Window, x: i32, y: i32, width: u32, height: u32) -> bool {
    let Ok(monitors) = window.available_monitors() else {
        return x > -10000 && y > -10000;
    };
    if monitors.is_empty() {
        return x > -10000 && y > -10000;
    }
    let w = width as i32;
    let h = height as i32;
    monitors.into_iter().any(|m| {
        let mp = m.position();
        let ms = m.size();
        let mw = ms.width as i32;
        let mh = ms.height as i32;
        let overlap_x = (x + w).min(mp.x + mw) - x.max(mp.x);
        let overlap_y = (y + h).min(mp.y + mh) - y.max(mp.y);
        overlap_x >= 80 && overlap_y >= 60
    })
}

fn resolve_target_url(raw: &str) -> String {
    if let Ok(parsed) = Url::parse(raw) {
        if parsed.host_str() == Some("www.google.com") && parsed.path() == "/url" {
            for (k, v) in parsed.query_pairs() {
                if (k == "q" || k == "url")
                    && (v.starts_with("https://") || v.starts_with("http://"))
                {
                    return v.into_owned();
                }
            }
        }
    }
    raw.to_string()
}

fn should_open_in_app(raw: &str) -> bool {
    let Ok(parsed) = Url::parse(raw) else {
        return true;
    };
    match parsed.scheme() {
        "about" | "blob" | "javascript" | "data" => true,
        "http" | "https" => matches!(
            parsed.host_str(),
            Some(
                "keep.google.com"
                    | "accounts.google.com"
                    | "accounts.youtube.com"
                    | "gds.google.com"
                    | "ogs.google.com"
            )
        ),
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn open_external_url(url: &str) {
    use windows::{
        core::{HSTRING, PCWSTR},
        Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    };

    let verb = HSTRING::from("open");
    let target = HSTRING::from(url);
    let _ = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
}

#[cfg(not(target_os = "windows"))]
fn open_external_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(target_os = "windows")]
fn is_autostart_enabled() -> bool {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(AUTOSTART_REG_KEY, KEY_READ) else {
        return false;
    };
    key.get_value::<String, _>(AUTOSTART_VALUE_NAME).is_ok()
}

#[cfg(target_os = "linux")]
fn linux_autostart_file() -> Option<PathBuf> {
    let config_dir = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(
        config_dir
            .join("autostart")
            .join("com.kyouztech.keepdesktop.desktop"),
    )
}

#[cfg(target_os = "linux")]
fn is_autostart_enabled() -> bool {
    linux_autostart_file().map(|p| p.exists()).unwrap_or(false)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn is_autostart_enabled() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" --minimized", exe.display());
        let (key, _) = hkcu
            .create_subkey(AUTOSTART_REG_KEY)
            .map_err(|e| e.to_string())?;
        key.set_value(AUTOSTART_VALUE_NAME, &command)
            .map_err(|e| e.to_string())?;
    } else if let Ok(key) = hkcu.open_subkey_with_flags(AUTOSTART_REG_KEY, KEY_WRITE) {
        if let Err(err) = key.delete_value(AUTOSTART_VALUE_NAME) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(err.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = linux_autostart_file()
        .ok_or_else(|| "Diretório de configuração XDG não encontrado".to_string())?;
    if enabled {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let exec_line = if std::env::var_os("FLATPAK_ID").is_some() {
            "flatpak run com.kyouztech.keepdesktop --minimized".to_string()
        } else {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            format!("\"{}\" --minimized", exe.display())
        };
        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Google Keep\n\
             Comment=Google Keep Desktop\n\
             Exec={exec_line}\n\
             Icon=com.kyouztech.keepdesktop\n\
             Terminal=false\n\
             Categories=Office;Utility;\n\
             X-GNOME-Autostart-enabled=true\n"
        );
        fs::write(&path, entry).map_err(|e| e.to_string())?;
    } else if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn set_autostart(_enabled: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn configure_windows_profile_and_navigate(
    webview: &wry::WebView,
    window: &Window,
    clear_cache_storage: bool,
) {
    use webview2_com::{
        ClearBrowsingDataCompletedHandler,
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Profile2, ICoreWebView2Profile3, ICoreWebView2_13,
            COREWEBVIEW2_BROWSING_DATA_KINDS, COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
            COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS,
            COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
        },
    };
    use windows_core::Interface;
    use wry::WebViewExtWindows;

    let core = webview.webview();
    unsafe {
        if let Ok(webview_13) = core.cast::<ICoreWebView2_13>() {
            if let Ok(profile) = webview_13.Profile() {
                if let Ok(profile_3) = profile.cast::<ICoreWebView2Profile3>() {
                    let _ = profile_3.SetPreferredTrackingPreventionLevel(
                        COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
                    );
                }

                if let Ok(profile_2) = profile.cast::<ICoreWebView2Profile2>() {
                    // Limpa Service Workers (e CacheStorage na migração) sem apagar cookies ou sessão Google.
                    let mut flags = COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS.0;
                    if clear_cache_storage {
                        flags |= COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0;
                    }
                    let data_kinds = COREWEBVIEW2_BROWSING_DATA_KINDS(flags);
                    let nav_window = window.clone();
                    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |_| {
                        with_keep_webview(&nav_window, |wv| {
                            let _ = wv.load_url(KEEP_URL);
                        });
                        Ok(())
                    }));
                    if profile_2.ClearBrowsingData(data_kinds, &handler).is_ok() {
                        return;
                    }
                }
            }
        }
    }

    let _ = webview.load_url(KEEP_URL);
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_profile_and_navigate(
    webview: &wry::WebView,
    _window: &Window,
    _clear_cache_storage: bool,
) {
    let _ = webview.load_url(KEEP_URL);
}

#[cfg(target_os = "windows")]
fn register_zoom_listener(
    webview: &wry::WebView,
    app: &AppHandle,
    zoom_status_item: MenuItem<tauri::Wry>,
) {
    use webview2_com::ZoomFactorChangedEventHandler;
    use wry::WebViewExtWindows;

    let app_handle = app.clone();
    let handler = ZoomFactorChangedEventHandler::create(Box::new(move |controller, _| {
        if let Some(ctrl) = controller {
            let mut factor = 1.0_f64;
            if unsafe { ctrl.ZoomFactor(&mut factor) }.is_ok() {
                let rounded = clamp_zoom(factor);
                let state = app_handle.state::<AppState>();
                let changed = if let Ok(mut cfg) = state.config.lock() {
                    if (cfg.zoom - rounded).abs() >= 0.005 {
                        cfg.zoom = rounded;
                        save_config(&state.config_path, &cfg);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if changed {
                    let _ = zoom_status_item.set_text(format_zoom_label(rounded));
                }
            }
        }
        Ok(())
    }));

    let mut token: i64 = 0;
    unsafe {
        let _ = webview
            .controller()
            .add_ZoomFactorChanged(&handler, &mut token);
    }
}

#[cfg(target_os = "linux")]
fn register_zoom_listener(
    webview: &wry::WebView,
    app: &AppHandle,
    zoom_status_item: MenuItem<tauri::Wry>,
) {
    use gtk::prelude::WidgetExt;
    use wry::WebViewExtUnix;

    let gtk_wv = webview.webview();

    let app_key = app.clone();
    let zoom_item_key = zoom_status_item.clone();
    gtk_wv.connect_key_press_event(move |_, event| {
        if !event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let kv = event.keyval();
        let current = app_key
            .state::<AppState>()
            .config
            .lock()
            .map(|cfg| cfg.zoom)
            .unwrap_or(1.0);
        if kv == gdk::keys::constants::plus
            || kv == gdk::keys::constants::equal
            || kv == gdk::keys::constants::KP_Add
            || kv == gdk::keys::constants::KP_Equal
        {
            apply_zoom(&app_key, &zoom_item_key, current + ZOOM_STEP);
            return glib::Propagation::Stop;
        }
        if kv == gdk::keys::constants::minus || kv == gdk::keys::constants::KP_Subtract {
            apply_zoom(&app_key, &zoom_item_key, current - ZOOM_STEP);
            return glib::Propagation::Stop;
        }
        if kv == gdk::keys::constants::_0 || kv == gdk::keys::constants::KP_0 {
            apply_zoom(&app_key, &zoom_item_key, 1.0);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });

    let app_scroll = app.clone();
    let zoom_item_scroll = zoom_status_item;
    gtk_wv.connect_scroll_event(move |_, event| {
        if !event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let current = app_scroll
            .state::<AppState>()
            .config
            .lock()
            .map(|cfg| cfg.zoom)
            .unwrap_or(1.0);
        match event.direction() {
            gdk::ScrollDirection::Up => {
                apply_zoom(&app_scroll, &zoom_item_scroll, current + ZOOM_STEP);
                glib::Propagation::Stop
            }
            gdk::ScrollDirection::Down => {
                apply_zoom(&app_scroll, &zoom_item_scroll, current - ZOOM_STEP);
                glib::Propagation::Stop
            }
            gdk::ScrollDirection::Smooth => {
                let (_, dy) = event.delta();
                if dy < -0.05 {
                    apply_zoom(&app_scroll, &zoom_item_scroll, current + ZOOM_STEP);
                    glib::Propagation::Stop
                } else if dy > 0.05 {
                    apply_zoom(&app_scroll, &zoom_item_scroll, current - ZOOM_STEP);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
            _ => glib::Propagation::Proceed,
        }
    });
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn register_zoom_listener(
    _webview: &wry::WebView,
    _app: &AppHandle,
    _zoom_status_item: MenuItem<tauri::Wry>,
) {
}

fn main() {
    let start_minimized = std::env::args().any(|arg| arg == "--minimized");

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &quick_note_shortcut() {
                        trigger_quick_note(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let data_dir = app.path().app_local_data_dir().ok();
            let qn_data_dir = data_dir.clone().map(|d| d.join("quick-note"));
            let config_path = data_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir)
                .join("desktop-config.json");

            let mut initial_cfg = load_config(&config_path);
            let system_autostart = is_autostart_enabled();
            if initial_cfg.autostart != system_autostart {
                initial_cfg.autostart = system_autostart;
            }
            // Garante que o comando salvo reflita a versão atual caso habilitado
            if initial_cfg.autostart {
                let _ = set_autostart(true);
            }

            let clear_cache_storage = !initial_cfg.sw_migrated_v2;
            if clear_cache_storage {
                initial_cfg.sw_migrated_v2 = true;
            }
            save_config(&config_path, &initial_cfg);

            let initial_zoom = initial_cfg.zoom;
            let initial_pin = initial_cfg.always_on_top;
            let initial_autostart = initial_cfg.autostart;
            let saved_size = initial_cfg.width.zip(initial_cfg.height);
            let saved_pos = initial_cfg.x.zip(initial_cfg.y);

            app.manage(AppState {
                config: Mutex::new(initial_cfg),
                config_path,
            });

            // Registra Ctrl+K exclusivamente para abrir a janela rápida de nova nota
            if let Err(err) = app.global_shortcut().register(quick_note_shortcut()) {
                eprintln!("Aviso: não foi possível registrar Ctrl+K: {err}");
            }

            // Cria janela nativa Tauri sem WebView Tauri (evita injeção de window.ipc / __TAURI_INTERNALS__
            // e filtros WebResourceRequested que quebram o bundle Closure do Google Keep)
            let mut window_builder = Window::builder(app, "main")
                .title("Google Keep")
                .inner_size(1100.0, 760.0)
                .resizable(true)
                .center()
                .visible(false)
                .theme(Some(Theme::Dark));

            if let Some(icon) = app.default_window_icon() {
                window_builder = window_builder.icon(icon.clone())?;
            }

            let window = window_builder.build()?;

            let (restore_w, restore_h) = match saved_size {
                Some((w, h)) if w >= 320 && h >= 240 => {
                    let _ = window.set_size(PhysicalSize::new(w, h));
                    (w, h)
                }
                _ => {
                    let s = window
                        .inner_size()
                        .unwrap_or_else(|_| PhysicalSize::new(1100, 760));
                    (s.width, s.height)
                }
            };

            if let Some((x, y)) = saved_pos {
                if is_position_on_any_monitor(&window, x, y, restore_w, restore_h) {
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                }
            }

            if initial_pin {
                let _ = window.set_always_on_top(true);
            }

            if !start_minimized {
                let _ = window.show();
                let _ = window.set_focus();
            }

            let zoom_status_item = MenuItem::with_id(
                app,
                "zoom_status",
                format_zoom_label(initial_zoom),
                false,
                None::<&str>,
            )?;

            let mut web_context = wry::WebContext::new(data_dir);
            let page_load_app = app.handle().clone();

            let webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_devtools(true)
                    .with_clipboard(true)
                    .with_hotkeys_zoom(true)
                    .with_new_window_req_handler(|url, _| {
                        let resolved = resolve_target_url(&url);
                        if should_open_in_app(&resolved) {
                            wry::NewWindowResponse::Allow
                        } else {
                            open_external_url(&resolved);
                            wry::NewWindowResponse::Deny
                        }
                    })
                    .with_on_page_load_handler(move |event, _| {
                        if matches!(event, PageLoadEvent::Finished) {
                            let state = page_load_app.state::<AppState>();
                            let zoom = state
                                .config
                                .lock()
                                .map(|cfg| cfg.zoom)
                                .unwrap_or(1.0);
                            if let Some(win) = page_load_app.get_window("main") {
                                with_keep_webview(&win, move |wv| {
                                    let _ = wv.zoom(zoom);
                                });
                            }
                        }
                    });

                #[cfg(target_os = "windows")]
                {
                    use wry::WebViewBuilderExtWindows;
                    builder = builder
                        .with_theme(wry::Theme::Dark)
                        .with_additional_browser_args(
                            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,ServiceWorkerStaticRouter,ThirdPartyStoragePartitioning,TrackingProtection3pcd",
                        )
                        .with_url("about:blank");
                }

                #[cfg(not(target_os = "windows"))]
                {
                    builder = builder.with_url(KEEP_URL);
                }

                #[cfg(target_os = "linux")]
                {
                    use wry::WebViewBuilderExtUnix;
                    let vbox = window.default_vbox()?;
                    let wv = builder.build_gtk(&vbox)?;
                    let _ = wv.set_visible(true);
                    wv
                }

                #[cfg(not(target_os = "linux"))]
                {
                    builder.build(&window)?
                }
            };

            let _ = webview.zoom(initial_zoom);
            register_zoom_listener(&webview, app.handle(), zoom_status_item.clone());

            KEEP_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(web_context);
            });
            KEEP_WEBVIEW.with(|cell| {
                if cfg!(target_os = "windows") {
                    configure_windows_profile_and_navigate(&webview, &window, clear_cache_storage);
                }
                *cell.borrow_mut() = Some(webview);
            });

            // Mini-janela dedicada para anotação rápida (Ctrl+K) sem exibir a janela principal
            let mut qn_builder = Window::builder(app, "quick-note")
                .title("Nova nota rápida — Google Keep")
                .inner_size(460.0, 290.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .center()
                .visible(false)
                .theme(Some(Theme::Dark));

            if let Some(icon) = app.default_window_icon() {
                qn_builder = qn_builder.icon(icon.clone())?;
            }

            let qn_window = qn_builder.build()?;

            let mut qn_web_context = wry::WebContext::new(qn_data_dir);
            let ipc_app = app.handle().clone();
            let qn_webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut qn_web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_clipboard(true)
                    .with_html(QUICK_NOTE_HTML)
                    .with_ipc_handler(move |req| {
                        if let Ok(msg) = serde_json::from_str::<QuickNoteIpc>(req.body()) {
                            let app_handle = ipc_app.clone();
                            let _ = ipc_app.run_on_main_thread(move || match msg {
                                QuickNoteIpc::Submit { title, body } => {
                                    submit_quick_note_to_keep(&app_handle, title, body);
                                }
                                QuickNoteIpc::Close => {
                                    hide_quick_note(&app_handle, true);
                                }
                                QuickNoteIpc::OpenMain => {
                                    hide_quick_note(&app_handle, false);
                                    if let Some(main_win) = app_handle.get_window("main") {
                                        show_and_focus_window(&main_win);
                                    }
                                }
                                QuickNoteIpc::Drag => {
                                    if let Some(qn_win) = app_handle.get_window("quick-note") {
                                        let _ = qn_win.start_dragging();
                                    }
                                }
                            });
                        }
                    });

                #[cfg(target_os = "windows")]
                {
                    use wry::WebViewBuilderExtWindows;
                    builder = builder.with_theme(wry::Theme::Dark);
                }

                #[cfg(target_os = "linux")]
                {
                    use wry::WebViewBuilderExtUnix;
                    let vbox = qn_window.default_vbox()?;
                    let wv = builder.build_gtk(&vbox)?;
                    let _ = wv.set_visible(true);
                    wv
                }

                #[cfg(not(target_os = "linux"))]
                {
                    builder.build(&qn_window)?
                }
            };

            QUICK_NOTE_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(qn_web_context);
            });
            QUICK_NOTE_WEBVIEW.with(|cell| {
                *cell.borrow_mut() = Some(qn_webview);
            });

            // Cria menu da bandeja do sistema (System Tray)
            let open_item =
                MenuItem::with_id(app, "open", "Abrir / Ocultar Keep", true, None::<&str>)?;
            let quick_note_item = MenuItem::with_id(
                app,
                "quick_note",
                "Nova nota rápida (Ctrl+K)",
                true,
                None::<&str>,
            )?;
            let reload_item =
                MenuItem::with_id(app, "reload", "Recarregar página", true, None::<&str>)?;
            let sep_top = PredefinedMenuItem::separator(app)?;
            let pin_item = CheckMenuItem::with_id(
                app,
                "pin",
                "Sempre no topo (Always on Top)",
                true,
                initial_pin,
                None::<&str>,
            )?;
            let autostart_item = CheckMenuItem::with_id(
                app,
                "autostart",
                AUTOSTART_MENU_LABEL,
                true,
                initial_autostart,
                None::<&str>,
            )?;

            let zoom_in_item =
                MenuItem::with_id(app, "zoom_in", "Aumentar (+10%)", true, None::<&str>)?;
            let zoom_out_item =
                MenuItem::with_id(app, "zoom_out", "Diminuir (-10%)", true, None::<&str>)?;
            let zoom_reset_item =
                MenuItem::with_id(app, "zoom_reset", "Restaurar (100%)", true, None::<&str>)?;
            let zoom_submenu = Submenu::with_id_and_items(
                app,
                "zoom_menu",
                "Zoom",
                true,
                &[
                    &zoom_status_item,
                    &zoom_in_item,
                    &zoom_out_item,
                    &zoom_reset_item,
                ],
            )?;

            let sep_mid = PredefinedMenuItem::separator(app)?;
            let devtools_item =
                MenuItem::with_id(app, "devtools", "Abrir DevTools (F12)", true, None::<&str>)?;
            let sep_bottom = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &open_item,
                    &quick_note_item,
                    &reload_item,
                    &sep_top,
                    &pin_item,
                    &autostart_item,
                    &zoom_submenu,
                    &sep_mid,
                    &devtools_item,
                    &sep_bottom,
                    &quit_item,
                ],
            )?;

            let pin_clone = pin_item.clone();
            let autostart_clone = autostart_item.clone();
            let zoom_status_clone = zoom_status_item.clone();

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "open" => toggle_window(app),
                    "quick_note" => trigger_quick_note(app),
                    "reload" => {
                        if let Some(win) = app.get_window("main") {
                            with_keep_webview(&win, |wv| {
                                let _ = wv.reload();
                            });
                        }
                    }
                    "pin" => {
                        let state = app.state::<AppState>();
                        let next = if let Ok(mut cfg) = state.config.lock() {
                            cfg.always_on_top = !cfg.always_on_top;
                            save_config(&state.config_path, &cfg);
                            cfg.always_on_top
                        } else {
                            false
                        };
                        let _ = pin_clone.set_checked(next);
                        if let Some(win) = app.get_window("main") {
                            let _ = win.set_always_on_top(next);
                        }
                    }
                    "autostart" => {
                        let state = app.state::<AppState>();
                        let desired = state
                            .config
                            .lock()
                            .map(|cfg| !cfg.autostart)
                            .unwrap_or(false);
                        match set_autostart(desired) {
                            Ok(()) => {
                                if let Ok(mut cfg) = state.config.lock() {
                                    cfg.autostart = desired;
                                    save_config(&state.config_path, &cfg);
                                }
                                let _ = autostart_clone.set_checked(desired);
                            }
                            Err(err) => {
                                eprintln!("Erro ao atualizar autostart: {err}");
                                let current = is_autostart_enabled();
                                let _ = autostart_clone.set_checked(current);
                            }
                        }
                    }
                    "zoom_in" => {
                        let current = app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|cfg| cfg.zoom)
                            .unwrap_or(1.0);
                        apply_zoom(app, &zoom_status_clone, current + ZOOM_STEP);
                    }
                    "zoom_out" => {
                        let current = app
                            .state::<AppState>()
                            .config
                            .lock()
                            .map(|cfg| cfg.zoom)
                            .unwrap_or(1.0);
                        apply_zoom(app, &zoom_status_clone, current - ZOOM_STEP);
                    }
                    "zoom_reset" => {
                        apply_zoom(app, &zoom_status_clone, 1.0);
                    }
                    "devtools" => {
                        if let Some(win) = app.get_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            with_keep_webview(&win, |wv| {
                                wv.open_devtools();
                            });
                        }
                    }
                    "quit" => {
                        save_state_config(app);
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            match tray_builder.build(app) {
                Ok(_) => TRAY_AVAILABLE.store(true, Ordering::Relaxed),
                Err(err) => {
                    eprintln!("Aviso: bandeja do sistema indisponível: {err}");
                    let _ = window.show();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "quick-note" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    hide_quick_note(window.app_handle(), true);
                    api.prevent_close();
                }
                return;
            }
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::Resized(size) => {
                    if !window.is_minimized().unwrap_or(false)
                        && window.is_visible().unwrap_or(false)
                        && size.width >= 320
                        && size.height >= 240
                    {
                        let state = window.app_handle().state::<AppState>();
                        if let Ok(mut cfg) = state.config.lock() {
                            cfg.width = Some(size.width);
                            cfg.height = Some(size.height);
                        };
                    }
                }
                WindowEvent::Moved(pos) => {
                    if !window.is_minimized().unwrap_or(false)
                        && window.is_visible().unwrap_or(false)
                        && pos.x > -10000
                        && pos.y > -10000
                    {
                        let state = window.app_handle().state::<AppState>();
                        if let Ok(mut cfg) = state.config.lock() {
                            cfg.x = Some(pos.x);
                            cfg.y = Some(pos.y);
                        };
                    }
                }
                WindowEvent::Focused(false) => {
                    save_state_config(window.app_handle());
                }
                // Oculta na bandeja quando disponível; caso contrário fecha normalmente
                WindowEvent::CloseRequested { api, .. } => {
                    save_state_config(window.app_handle());
                    if TRAY_AVAILABLE.load(Ordering::Relaxed) {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("Erro ao executar aplicação Tauri");
}
