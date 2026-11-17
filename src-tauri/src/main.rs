#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
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
const MIN_ZOOM: f64 = 0.5;
const MAX_ZOOM: f64 = 2.0;
const ZOOM_STEP: f64 = 0.1;

#[cfg(target_os = "windows")]
const AUTOSTART_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const AUTOSTART_VALUE_NAME: &str = "GoogleKeepDesktop";

const QUICK_NOTE_JS: &str = r#"(function () {
  function isVisible(el) {
    if (!el) return false;
    var style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') return false;
    var rect = el.getBoundingClientRect();
    return rect.width > 40 && rect.height > 15;
  }

  function findComposerBox() {
    var labelled = document.querySelectorAll(
      'div[role="textbox"][aria-label*="Criar uma nota" i], div[role="textbox"][aria-label*="Take a note" i], [aria-label="Criar uma nota..."], [aria-label="Take a note..."]'
    );
    for (var i = 0; i < labelled.length; i++) {
      if (!labelled[i].closest('[role="dialog"]') && isVisible(labelled[i])) {
        return labelled[i];
      }
    }

    var boxes = document.querySelectorAll('div[role="textbox"][contenteditable="true"]');
    for (var j = 0; j < boxes.length; j++) {
      if (!boxes[j].closest('[role="dialog"]') && isVisible(boxes[j])) {
        return boxes[j];
      }
    }
    return null;
  }

  function triggerComposer() {
    var box = findComposerBox();
    if (box) {
      box.click();
      box.focus();
      return true;
    }
    return false;
  }

  if (!triggerComposer()) {
    if (window.location.hash && window.location.hash !== '#home') {
      window.location.hash = '#home';
      setTimeout(triggerComposer, 300);
    } else {
      var active = document.activeElement;
      var inInput =
        active &&
        (active.tagName === 'INPUT' ||
          active.tagName === 'TEXTAREA' ||
          active.isContentEditable);
      if (!inInput && !document.querySelector('[role="dialog"]')) {
        document.dispatchEvent(
          new KeyboardEvent('keydown', {
            key: 'c',
            code: 'KeyC',
            keyCode: 67,
            which: 67,
            bubbles: true,
          })
        );
      }
    }
  }
})();"#;

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
}

fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK)
}

fn quick_note_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyN)
}

fn clamp_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return 1.0;
    }
    (zoom.clamp(MIN_ZOOM, MAX_ZOOM) * 10.0).round() / 10.0
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
    }
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

fn trigger_quick_note(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        with_keep_webview(&window, |wv| {
            let _ = wv.focus();
            let _ = wv.evaluate_script(QUICK_NOTE_JS);
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
fn is_windows_autostart_enabled() -> bool {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(AUTOSTART_REG_KEY, KEY_READ) else {
        return false;
    };
    key.get_value::<String, _>(AUTOSTART_VALUE_NAME).is_ok()
}

#[cfg(not(target_os = "windows"))]
fn is_windows_autostart_enabled() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn set_windows_autostart(enabled: bool) -> Result<(), String> {
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

#[cfg(not(target_os = "windows"))]
fn set_windows_autostart(_enabled: bool) -> Result<(), String> {
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

fn main() {
    let start_minimized = std::env::args().any(|arg| arg == "--minimized");

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &toggle_shortcut() {
                        toggle_window(app);
                    } else if shortcut == &quick_note_shortcut() {
                        trigger_quick_note(app);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let data_dir = app.path().app_local_data_dir().ok();
            let config_path = data_dir
                .clone()
                .unwrap_or_else(std::env::temp_dir)
                .join("desktop-config.json");

            let mut initial_cfg = load_config(&config_path);
            let registry_autostart = is_windows_autostart_enabled();
            if initial_cfg.autostart != registry_autostart {
                initial_cfg.autostart = registry_autostart;
            }
            // Garante que o caminho no registro reflita a versão atual caso habilitado
            if initial_cfg.autostart {
                let _ = set_windows_autostart(true);
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

            // Registra atalhos globais sem abortar caso já estejam ocupados no sistema
            if let Err(err) = app.global_shortcut().register(toggle_shortcut()) {
                eprintln!("Aviso: não foi possível registrar Ctrl+Alt+K: {err}");
            }
            if let Err(err) = app.global_shortcut().register(quick_note_shortcut()) {
                eprintln!("Aviso: não foi possível registrar Ctrl+Alt+N: {err}");
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

            let mut web_context = wry::WebContext::new(data_dir);
            let page_load_app = app.handle().clone();

            let webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_devtools(true)
                    .with_clipboard(true)
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

                builder.build(&window)?
            };

            let _ = webview.zoom(initial_zoom);

            KEEP_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(web_context);
            });
            KEEP_WEBVIEW.with(|cell| {
                if cfg!(target_os = "windows") {
                    configure_windows_profile_and_navigate(&webview, &window, clear_cache_storage);
                }
                *cell.borrow_mut() = Some(webview);
            });

            // Cria menu da bandeja do sistema (System Tray)
            let open_item =
                MenuItem::with_id(app, "open", "Abrir / Ocultar (Ctrl+Alt+K)", true, None::<&str>)?;
            let quick_note_item = MenuItem::with_id(
                app,
                "quick_note",
                "Nova nota rápida (Ctrl+Alt+N)",
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
                "Iniciar com o Windows",
                true,
                initial_autostart,
                None::<&str>,
            )?;

            let zoom_status_item = MenuItem::with_id(
                app,
                "zoom_status",
                format_zoom_label(initial_zoom),
                false,
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
                        match set_windows_autostart(desired) {
                            Ok(()) => {
                                if let Ok(mut cfg) = state.config.lock() {
                                    cfg.autostart = desired;
                                    save_config(&state.config_path, &cfg);
                                }
                                let _ = autostart_clone.set_checked(desired);
                            }
                            Err(err) => {
                                eprintln!("Erro ao atualizar autostart no Windows: {err}");
                                let current = is_windows_autostart_enabled();
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

            tray_builder.build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
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
                    }
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
                    }
                }
            }
            WindowEvent::Focused(false) => {
                save_state_config(window.app_handle());
            }
            // Oculta na bandeja em vez de fechar o app ao clicar no X
            WindowEvent::CloseRequested { api, .. } => {
                save_state_config(window.app_handle());
                let _ = window.hide();
                api.prevent_close();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("Erro ao executar aplicação Tauri");
}
