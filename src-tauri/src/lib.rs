mod application;
mod commands;
mod domain;
mod provider;
mod secrets;
mod storage;
mod window_shell;

use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = storage::Store::open(&directory.join("companion.sqlite3"))
                .map_err(|e| std::io::Error::other(e.message))?;
            let provider = provider::DeepSeek::new().map_err(|e| std::io::Error::other(e.message))?;
            app.manage(Arc::new(application::Application {
                core: Mutex::new(application::Core {store,resting:false,active:None,revision:0,early_cancels:Default::default()}),
                provider: Box::new(provider),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::get_snapshot,commands::save_settings,commands::set_pet_state,commands::cancel_reply,commands::clear_conversation,commands::send_message,window_shell::set_shell_mode])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
