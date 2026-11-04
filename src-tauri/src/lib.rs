mod application;
mod commands;
mod domain;
mod provider;
mod secrets;
mod storage;
mod window_shell;
mod display;
mod bubble;

use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("pet") {
                let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus();
                if let Err(error) = bubble::refresh(app) { eprintln!("{}", error.message); }
            }
        }))
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = storage::Store::open(&directory.join("companion.sqlite3"))
                .map_err(|e| std::io::Error::other(e.message))?;
            let scales = store.display_scale().map_err(|e| std::io::Error::other(e.message))?;
            let quiet = store.quiet().map_err(|e| std::io::Error::other(e.message))?;
            let zoom = scales.pet_percent as f64 / 100.0;
            let provider = provider::DeepSeek::new().map_err(|e| std::io::Error::other(e.message))?;
            app.manage(Arc::new(application::Application {
                core: Mutex::new(application::Core {store,resting:false,active:None,revision:0,early_cancels:Default::default()}),
                provider: Box::new(provider),
            }));
            app.manage(Mutex::new(window_shell::ShellState { scales, ..Default::default() }));
            app.manage(Mutex::new(bubble::BubbleState::new(quiet)));
            let pet = tauri::WebviewWindowBuilder::new(app, "pet", tauri::WebviewUrl::App("index.html".into()))
                .title("Hello World · 桌宠").inner_size(180.0 * zoom, 180.0 * zoom).visible(false)
                .transparent(true).decorations(false).shadow(false)
                .always_on_top(true).resizable(false).build()?;
            pet.set_zoom(zoom)?;
            let bubble = tauri::WebviewWindowBuilder::new(app, "bubble", tauri::WebviewUrl::App("index.html".into()))
                .title("Hello World · 气泡").inner_size(132.0,58.0).visible(false)
                .transparent(true).decorations(false).shadow(false).always_on_top(true)
                .resizable(false).skip_taskbar(true).focused(false).build()?;
            bubble.set_focusable(false)?;
            bubble.set_ignore_cursor_events(true)?;
            app.get_webview_window("main").expect("main panel missing").set_zoom(scales.panel_percent as f64 / 100.0)?;
            pet.show()?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "pet" && matches!(event, tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } | tauri::WindowEvent::Focused(_)) {
                if let Err(error) = bubble::refresh(window.app_handle()) { eprintln!("{}",error.message); }
            }
            if window.label() == "pet" && matches!(event, tauri::WindowEvent::Moved(_)) {
                let visible = {
                    let state = window.state::<Mutex<window_shell::ShellState>>();
                    let mut state = state.lock().expect("shell state lock poisoned");
                    let from_scale = matches!(event, tauri::WindowEvent::Moved(position) if Some(*position) == state.scale_position);
                    if !from_scale && !state.scaling { state.scale_position = None; }
                    !from_scale && !state.scaling && state.mode != window_shell::ShellMode::Idle
                };
                if visible {
                    if let Err(error) = window_shell::apply_mode(window.app_handle(), window_shell::ShellMode::Idle) {
                        eprintln!("{}", error.message);
                    }
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    if let Err(error) = window_shell::apply_mode(window.app_handle(), window_shell::ShellMode::Idle) {
                        eprintln!("{}", error.message);
                    }
                } else if window.label() == "pet" { window.app_handle().exit(0); }
            }
        })
        .invoke_handler(tauri::generate_handler![commands::get_snapshot,commands::get_pet_status,commands::save_settings,commands::set_pet_state,commands::cancel_reply,commands::clear_conversation,commands::send_message,window_shell::set_shell_mode,window_shell::panel_ready,window_shell::shell_action,display::get_display_scale,display::set_display_scale,bubble::bubble_ready,bubble::register_bubble_model,bubble::get_bubble_calibration,bubble::start_bubble_calibration,bubble::finish_bubble_calibration])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
