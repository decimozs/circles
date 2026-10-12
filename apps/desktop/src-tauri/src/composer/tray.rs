//! The menu bar item: keeps Circles running after its window is closed, so schedules and triggers
//! go on, and gives a way to look at what waits and to pause or stop the Composers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::AppState;

/// Set once the menu bar item exists. Without it, closing the window quits as usual.
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATUS: OnceLock<MenuItem<Wry>> = OnceLock::new();
static PAUSE: OnceLock<MenuItem<Wry>> = OnceLock::new();

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// Brings the window back from where closing it put it.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Makes the item. Called when the setting is on, as Circles starts.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Circles", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "Nothing is waiting", false, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause Composers", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop_all", "Stop all Composers", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Circles", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &status,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let _ = STATUS.set(status);
    let _ = PAUSE.set(pause);

    let mut builder = TrayIconBuilder::with_id("circles")
        .tooltip("Circles")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let state = app.state::<AppState>();
            match event.id().as_ref() {
                "open" => show_window(app),
                "pause" => {
                    if let Ok(dispatcher) = state.composers.dispatcher() {
                        let now = dispatcher.paused.load(Ordering::SeqCst);
                        dispatcher.set_paused(!now);
                    }
                    refresh(app);
                }
                "stop_all" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        if let Err(e) = crate::commands::composer::stop_everything(&state).await {
                            tracing::warn!(error = %e, "cannot stop the Composers");
                        }
                        refresh(&app);
                    });
                }
                "quit" => {
                    let busy = state.running_work();
                    if busy > 0 {
                        // The window asks, as it does for Cmd+Q.
                        show_window(app);
                        let _ = app.emit("app:confirm-quit", busy);
                    } else {
                        app.exit(0);
                    }
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    ACTIVE.store(true, Ordering::SeqCst);
    Ok(())
}

/// Brings the item's lines up to date with what waits and whether Composers are paused.
pub fn refresh(app: &AppHandle) {
    if !is_active() {
        return;
    }
    let state = app.state::<AppState>();
    let waiting = state
        .composers
        .store
        .approvals(None, None, true, 1000)
        .map(|a| a.len())
        .unwrap_or(0);
    let paused = state
        .composers
        .dispatcher()
        .map(|d| d.paused.load(Ordering::SeqCst))
        .unwrap_or(false);
    if let Some(item) = STATUS.get() {
        let _ = item.set_text(match waiting {
            0 => "Nothing is waiting".to_string(),
            1 => "1 thing is waiting for you".to_string(),
            n => format!("{n} things are waiting for you"),
        });
    }
    if let Some(item) = PAUSE.get() {
        let _ = item.set_text(if paused {
            "Resume Composers"
        } else {
            "Pause Composers"
        });
    }
    if let Some(tray) = app.tray_by_id("circles") {
        let _ = tray.set_tooltip(Some(if waiting > 0 {
            format!("Circles: {waiting} waiting for you")
        } else {
            "Circles".to_string()
        }));
    }
}
