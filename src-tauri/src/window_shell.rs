use crate::domain::{AppError, DisplayScale};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode { #[default] Idle, Menu, Chat, Settings, Privacy }

pub struct ShellState {
    pub mode: ShellMode, pub ready: bool, pub scales: DisplayScale, pub scaling: bool,
    pub scale_position: Option<tauri::PhysicalPosition<i32>>,
}
impl Default for ShellState {
    fn default() -> Self { Self { mode: ShellMode::Idle, ready: false, scales: DisplayScale::default(), scaling: false, scale_position: None } }
}

// Only the panel is placed/clamped. The pet's screen rectangle is read-only.
fn panel_position(pet: (i32, i32, u32, u32), size: (u32, u32), area: (i32, i32, u32, u32), gap: i32) -> (i32, i32) {
    let (x, y, w, h) = (pet.0 as i64, pet.1 as i64, pet.2 as i64, pet.3 as i64);
    let (pw, ph, gap) = (size.0 as i64, size.1 as i64, gap as i64);
    let (left, top) = (area.0 as i64, area.1 as i64);
    let (right, bottom) = (left + area.2 as i64, top + area.3 as i64);
    let centred = x + (w - pw) / 2;
    let preferred = if y - gap - ph >= top { (centred, y - gap - ph) }
        else if y + h + gap + ph <= bottom { (centred, y + h + gap) }
        else if x + w + gap + pw <= right { (x + w + gap, y) }
        else if x - gap - pw >= left { (x - gap - pw, y) }
        else { (centred, y - gap - ph) };
    (preferred.0.clamp(left, (right - pw).max(left)) as i32,
     preferred.1.clamp(top, (bottom - ph).max(top)) as i32)
}

fn failure(_: impl std::fmt::Display) -> AppError { AppError::new("window", "面板操作未完成，请重试。") }

pub fn apply_mode(app: &AppHandle, mode: ShellMode) -> Result<(), AppError> {
    let panel = app.get_webview_window("main").ok_or_else(|| failure("panel"))?;
    let pet = app.get_webview_window("pet").ok_or_else(|| failure("pet"))?;
    let before = (pet.outer_position().map_err(failure)?, pet.inner_size().map_err(failure)?);
    let (ready, percent) = {
        let state = app.state::<Mutex<ShellState>>();
        let state = state.lock().map_err(failure)?;
        (state.ready, state.scales.panel_percent)
    };
    if mode == ShellMode::Idle {
        panel.hide().map_err(failure)?;
    } else if ready {
        let monitor = pet.current_monitor().map_err(failure)?.ok_or_else(|| failure("monitor"))?;
        let area = monitor.work_area();
        let scale = pet.scale_factor().map_err(failure)? * percent as f64 / 100.0;
        let logical = if mode == ShellMode::Menu { (300.0, 180.0) } else { (440.0, 380.0) };
        let size = (((logical.0 * scale).round() as u32).min(area.size.width),
                    ((logical.1 * scale).round() as u32).min(area.size.height));
        let pos = panel_position((before.0.x, before.0.y, before.1.width, before.1.height), size,
            (area.position.x, area.position.y, area.size.width, area.size.height), (6.0 * scale).round() as i32);
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOCOPYBITS};
            let hwnd = panel.hwnd().map_err(failure)?;
            let ok = unsafe { SetWindowPos(hwnd.0 as _, std::ptr::null_mut(), pos.0, pos.1,
                size.0 as i32, size.1 as i32, SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOCOPYBITS) };
            if ok == 0 { return Err(failure("SetWindowPos")); }
        }
        #[cfg(not(windows))]
        {
            panel.set_size(tauri::PhysicalSize::new(size.0, size.1)).map_err(failure)?;
            panel.set_position(tauri::PhysicalPosition::new(pos.0, pos.1)).map_err(failure)?;
        }
        panel.show().map_err(failure)?;
        panel.set_focus().map_err(failure)?;
    }
    app.state::<Mutex<ShellState>>().lock().map_err(failure)?.mode = mode;
    app.emit_to("main", "shell-view", mode).map_err(failure)?;
    app.emit_to("pet", "shell-view", mode).map_err(failure)?;
    #[cfg(debug_assertions)]
    {
        let after = (pet.outer_position().map_err(failure)?, pet.inner_size().map_err(failure)?);
        eprintln!("shell-transition {mode:?}: pet_before={before:?} pet_after={after:?}");
        debug_assert_eq!(before, after, "Panel transitions must never move or resize the pet");
    }
    Ok(())
}

#[tauri::command]
pub async fn set_shell_mode(window: WebviewWindow, mode: ShellMode) -> Result<(), AppError> {
    if window.label() != "main" && window.label() != "pet" {
        return Err(AppError::new("permission", "此窗口不能改变桌宠布局。"));
    }
    let (done, result) = tokio::sync::oneshot::channel();
    let app = window.app_handle().clone();
    window.run_on_main_thread(move || { let _ = done.send(apply_mode(&app, mode)); }).map_err(failure)?;
    result.await.map_err(failure)?
}

#[tauri::command]
pub async fn panel_ready(window: WebviewWindow) -> Result<(), AppError> {
    if window.label() != "main" { return Err(AppError::new("permission", "此窗口不是面板。")); }
    let (done, result) = tokio::sync::oneshot::channel();
    let app = window.app_handle().clone();
    window.run_on_main_thread(move || {
        let ready = || -> Result<(), AppError> {
            let mode = {
                let state = app.state::<Mutex<ShellState>>();
                let mut state = state.lock().map_err(failure)?;
                state.ready = true;
                state.mode
            };
            apply_mode(&app, mode)
        };
        let _ = done.send(ready());
    }).map_err(failure)?;
    result.await.map_err(failure)?
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellAction { Minimize, Exit }

#[tauri::command]
pub async fn shell_action(window: WebviewWindow, action: ShellAction) -> Result<(), AppError> {
    if window.label() != "main" { return Err(AppError::new("permission", "此窗口不能执行此操作。")); }
    set_shell_mode(window.clone(), ShellMode::Idle).await?;
    match action {
        ShellAction::Minimize => window.app_handle().get_webview_window("pet").ok_or_else(|| failure("pet"))?.minimize().map_err(failure),
        ShellAction::Exit => { window.app_handle().exit(0); Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::panel_position;
    #[test]
    fn places_panel_above_or_below_without_changing_pet() {
        assert_eq!(panel_position((800, 800, 180, 180), (440, 380), (0, 0, 1920, 1040), 6), (670, 414));
        assert_eq!(panel_position((800, 0, 180, 180), (440, 380), (0, 0, 1920, 1040), 6), (670, 186));
    }
    #[test]
    fn handles_edges_negative_coordinates_and_small_work_areas() {
        assert_eq!(panel_position((-1920, 0, 180, 180), (440, 380), (-1920, 0, 1920, 1040), 6), (-1920, 186));
        assert_eq!(panel_position((1740, 860, 180, 180), (440, 380), (0, 0, 1920, 1040), 6), (1480, 474));
        assert_eq!(panel_position((0, 150, 180, 180), (440, 380), (0, 0, 1920, 480), 6), (186, 100));
        assert_eq!(panel_position((0, 0, 180, 180), (300, 200), (0, 0, 300, 200), 6), (0, 0));
    }
}
