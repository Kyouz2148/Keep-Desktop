#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Url, WebviewWindow, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const KEEP_URL: &str = "https://keep.google.com";
static ALWAYS_ON_TOP: AtomicBool = AtomicBool::new(false);

fn keep_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK)
}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_visible && !is_minimized {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }
}

#[cfg(target_os = "windows")]
fn configure_webview_and_navigate(window: &WebviewWindow, target_url: Url) {
    use webview2_com::{
        ClearBrowsingDataCompletedHandler,
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Profile2, ICoreWebView2Profile3, ICoreWebView2_13, ICoreWebView2_22,
            COREWEBVIEW2_BROWSING_DATA_KINDS, COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
            COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS,
            COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
        },
    };
    use windows_core::{Interface, HSTRING};

    let nav_window = window.clone();
    let res = window.with_webview(move |wv| unsafe {
        let controller = wv.controller();
        let Ok(webview) = controller.CoreWebView2() else {
            let _ = nav_window.navigate(target_url);
            return;
        };

        // Remove wry's custom-protocol WebResourceRequested filters before loading Keep
        // to avoid MicrosoftEdge/WebView2Feedback#5764 (Service Workers using
        // InstallEvent.addRoutes getting stuck in STARTING when any filter is active).
        let filters = [
            "http://tauri.localhost/*",
            "https://tauri.localhost/*",
            "http://ipc.localhost/*",
            "https://ipc.localhost/*",
        ];
        if let Ok(webview_22) = webview.cast::<ICoreWebView2_22>() {
            for uri in filters {
                let filter = HSTRING::from(uri);
                let _ = webview_22.RemoveWebResourceRequestedFilterWithRequestSourceKinds(
                    &filter,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                );
            }
        } else {
            for uri in filters {
                let filter = HSTRING::from(uri);
                let _ = webview.RemoveWebResourceRequestedFilter(
                    &filter,
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                );
            }
        }

        if let Ok(webview_13) = webview.cast::<ICoreWebView2_13>() {
            if let Ok(profile) = webview_13.Profile() {
                if let Ok(profile_3) = profile.cast::<ICoreWebView2Profile3>() {
                    let _ = profile_3.SetPreferredTrackingPreventionLevel(
                        COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
                    );
                }

                if let Ok(profile_2) = profile.cast::<ICoreWebView2Profile2>() {
                    // Clear stale/stuck Service Workers and CacheStorage without touching
                    // cookies or local storage so the active Google session stays logged in.
                    let data_kinds = COREWEBVIEW2_BROWSING_DATA_KINDS(
                        COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS.0
                            | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0,
                    );
                    let callback_window = nav_window.clone();
                    let callback_url = target_url.clone();
                    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |_| {
                        let _ = callback_window.navigate(callback_url.clone());
                        Ok(())
                    }));
                    if profile_2.ClearBrowsingData(data_kinds, &handler).is_ok() {
                        return;
                    }
                }
            }
        }

        let _ = nav_window.navigate(target_url);
    });

    if res.is_err() {
        if let Ok(fallback) = Url::parse(KEEP_URL) {
            let _ = window.navigate(fallback);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_webview_and_navigate(window: &WebviewWindow, target_url: Url) {
    let _ = window.navigate(target_url);
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && shortcut == &keep_shortcut() {
                        toggle_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Registra atalho global Ctrl + Alt + K sem abortar o app caso já esteja em uso
            if let Err(err) = app.global_shortcut().register(keep_shortcut()) {
                eprintln!("Aviso: não foi possível registrar o atalho Ctrl+Alt+K: {err}");
            }

            if let Some(window) = app.get_webview_window("main") {
                if let Ok(keep_url) = Url::parse(KEEP_URL) {
                    configure_webview_and_navigate(&window, keep_url);
                }
            }

            // Cria menu da bandeja do sistema (System Tray)
            let open_item =
                MenuItem::with_id(app, "open", "Abrir Google Keep", true, None::<&str>)?;
            let reload_item =
                MenuItem::with_id(app, "reload", "Recarregar página", true, None::<&str>)?;
            let pin_item = CheckMenuItem::with_id(
                app,
                "pin",
                "Sempre no topo (Always on Top)",
                true,
                false,
                None::<&str>,
            )?;
            let devtools_item =
                MenuItem::with_id(app, "devtools", "Abrir DevTools (F12)", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open_item,
                    &reload_item,
                    &pin_item,
                    &devtools_item,
                    &separator,
                    &quit_item,
                ],
            )?;

            let pin_clone = pin_item.clone();

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "open" => toggle_window(app),
                    "reload" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.reload();
                        }
                    }
                    "pin" => {
                        let next = !ALWAYS_ON_TOP.load(Ordering::Relaxed);
                        ALWAYS_ON_TOP.store(next, Ordering::Relaxed);
                        let _ = pin_clone.set_checked(next);
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.set_always_on_top(next);
                        }
                    }
                    "devtools" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                            win.open_devtools();
                        }
                    }
                    "quit" => {
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
        .on_window_event(|window, event| {
            // Oculta na bandeja em vez de fechar o app ao clicar no X
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("Erro ao executar aplicação Tauri");
}
