#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, Ordering},
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const KEEP_URL: &str = "https://keep.google.com";
static ALWAYS_ON_TOP: AtomicBool = AtomicBool::new(false);

thread_local! {
    static KEEP_CONTEXT: RefCell<Option<wry::WebContext>> = const { RefCell::new(None) };
    static KEEP_WEBVIEW: RefCell<Option<wry::WebView>> = const { RefCell::new(None) };
}

fn keep_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyK)
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

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        let is_minimized = window.is_minimized().unwrap_or(false);
        if is_visible && !is_minimized {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            with_keep_webview(&window, |wv| {
                let _ = wv.focus();
            });
        }
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_profile_and_navigate(webview: &wry::WebView, window: &Window) {
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
                    // Clear potentially stuck Service Workers and CacheStorage from prior builds
                    // without touching cookies or local storage so the Google session stays active.
                    let data_kinds = COREWEBVIEW2_BROWSING_DATA_KINDS(
                        COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS.0
                            | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0,
                    );
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
fn configure_windows_profile_and_navigate(webview: &wry::WebView, _window: &Window) {
    let _ = webview.load_url(KEEP_URL);
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

            // Cria janela nativa Tauri sem WebView Tauri (evita injeção de window.ipc / __TAURI_INTERNALS__
            // e filtros WebResourceRequested que quebram o bundle Closure do Google Keep)
            let mut window_builder = Window::builder(app, "main")
                .title("Google Keep")
                .inner_size(1100.0, 760.0)
                .resizable(true)
                .center();

            if let Some(icon) = app.default_window_icon() {
                window_builder = window_builder.icon(icon.clone())?;
            }

            let window = window_builder.build()?;

            let data_dir = app.path().app_local_data_dir().ok();
            let mut web_context = wry::WebContext::new(data_dir);

            let webview = {
                let mut builder = wry::WebViewBuilder::new_with_web_context(&mut web_context)
                    .with_background_color((0x20, 0x21, 0x24, 0xff))
                    .with_devtools(true)
                    .with_clipboard(true)
                    .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Allow);

                #[cfg(target_os = "windows")]
                {
                    use wry::WebViewBuilderExtWindows;
                    builder = builder
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

            KEEP_CONTEXT.with(|cell| {
                *cell.borrow_mut() = Some(web_context);
            });
            KEEP_WEBVIEW.with(|cell| {
                if cfg!(target_os = "windows") {
                    configure_windows_profile_and_navigate(&webview, &window);
                }
                *cell.borrow_mut() = Some(webview);
            });

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
                        if let Some(win) = app.get_window("main") {
                            with_keep_webview(&win, |wv| {
                                let _ = wv.reload();
                            });
                        }
                    }
                    "pin" => {
                        let next = !ALWAYS_ON_TOP.load(Ordering::Relaxed);
                        ALWAYS_ON_TOP.store(next, Ordering::Relaxed);
                        let _ = pin_clone.set_checked(next);
                        if let Some(win) = app.get_window("main") {
                            let _ = win.set_always_on_top(next);
                        }
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
