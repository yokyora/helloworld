use std::sync::{Arc, Mutex};
use tauri::{Manager, State, WebviewWindow};
use crate::{application::{access, Application}, domain::{AppError, DisplayScale}, window_shell::{apply_mode, ShellState}};

fn window_error(_: impl std::fmt::Display) -> AppError { AppError::new("display", "界面缩放未完成，请重试。") }

fn validate(percent: u16) -> Result<(), AppError> {
    if matches!(percent, 75 | 100 | 125 | 150 | 175 | 200) { Ok(()) }
    else { Err(AppError::new("display", "请选择 75%～200% 的可用缩放档位。")) }
}

fn pet_position(old: (i32, i32, u32, u32), size: u32, area: (i32, i32, u32, u32)) -> (i32, i32) {
    let x = old.0 as i64 + old.2 as i64 / 2 - size as i64 / 2;
    let y = old.1 as i64 + old.3 as i64 - size as i64;
    (x.clamp(area.0 as i64, area.0 as i64 + area.2.saturating_sub(size) as i64) as i32,
     y.clamp(area.1 as i64, area.1 as i64 + area.3.saturating_sub(size) as i64) as i32)
}

async fn apply(window: &WebviewWindow, scales: DisplayScale, previous: DisplayScale) -> Result<(), AppError> {
    let app = window.app_handle().clone();
    let (done, result) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || {
        let update = || -> Result<(), AppError> {
            let pet = app.get_webview_window("pet").ok_or_else(|| window_error("pet"))?;
            let panel = app.get_webview_window("main").ok_or_else(|| window_error("panel"))?;
            if scales.pet_percent != previous.pet_percent {
                let old_pos = pet.outer_position().map_err(window_error)?;
                let old_size = pet.inner_size().map_err(window_error)?;
                let monitor = pet.current_monitor().map_err(window_error)?.ok_or_else(|| window_error("monitor"))?;
                let area = monitor.work_area();
                let zoom = scales.pet_percent as f64 / 100.0;
                let size = (180.0 * pet.scale_factor().map_err(window_error)? * zoom).round() as u32;
                if size > area.size.width || size > area.size.height {
                    return Err(AppError::new("display", "当前屏幕无法完整显示此大小的角色，请选择较小的缩放。"));
                }
                let pos = pet_position((old_pos.x, old_pos.y, old_size.width, old_size.height), size,
                    (area.position.x, area.position.y, area.size.width, area.size.height));
                // Native Moved events can arrive after this UI-thread task returns.
                app.state::<Mutex<ShellState>>().lock().map_err(window_error)?.scale_position =
                    Some(tauri::PhysicalPosition::new(pos.0, pos.1));
                pet.set_zoom(zoom).map_err(window_error)?;
                #[cfg(windows)]
                {
                    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOCOPYBITS};
                    let hwnd = pet.hwnd().map_err(window_error)?;
                    if unsafe { SetWindowPos(hwnd.0 as _, std::ptr::null_mut(), pos.0, pos.1, size as i32, size as i32,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOCOPYBITS) } == 0 { return Err(window_error("SetWindowPos")); }
                }
                #[cfg(not(windows))]
                {
                    pet.set_size(tauri::PhysicalSize::new(size, size)).map_err(window_error)?;
                    pet.set_position(tauri::PhysicalPosition::new(pos.0, pos.1)).map_err(window_error)?;
                }
            }
            if scales.panel_percent != previous.panel_percent {
                panel.set_zoom(scales.panel_percent as f64 / 100.0).map_err(window_error)?;
            }
            let mode = {
                let state = app.state::<Mutex<ShellState>>();
                let mut state = state.lock().map_err(window_error)?;
                state.scales = scales;
                state.mode
            };
            apply_mode(&app, mode)?;
            Ok(())
        };
        let outcome = (|| {
            app.state::<Mutex<ShellState>>().lock().map_err(window_error)?.scaling = true;
            let result = update();
            app.state::<Mutex<ShellState>>().lock().map_err(window_error)?.scaling = false;
            result
        })();
        let _ = done.send(outcome);
    }).map_err(window_error)?;
    result.await.map_err(window_error)?
}

#[tauri::command]
pub async fn get_display_scale(window: WebviewWindow, app: State<'_, Arc<Application>>) -> Result<DisplayScale, AppError> {
    if window.label() != "main" { return Err(AppError::new("permission", "此窗口不能读取显示设置。")); }
    access(&app, |c| c.store.display_scale()).await
}

#[tauri::command]
pub async fn set_display_scale(window: WebviewWindow, app: State<'_, Arc<Application>>, scales: DisplayScale) -> Result<DisplayScale, AppError> {
    if window.label() != "main" { return Err(AppError::new("permission", "此窗口不能修改显示设置。")); }
    validate(scales.pet_percent)?;
    validate(scales.panel_percent)?;
    let previous = access(&app, |c| c.store.display_scale()).await?;
    if scales == previous { return Ok(scales); }
    let outcome = match apply(&window, scales, previous).await {
        Ok(()) => access(&app, move |c| c.store.save_display_scale(scales)).await,
        Err(error) => Err(error),
    };
    if let Err(error) = outcome {
        if apply(&window, previous, scales).await.is_err() {
            return Err(AppError::new("display", "缩放未保存，且界面恢复失败，请重新启动桌宠。"));
        }
        return Err(error);
    }
    Ok(scales)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_round_trip_keeps_bottom_centre_and_clamps_only_at_edges() {
        let area = (0, 0, 1920, 1040);
        for size in [135, 180, 225, 270, 315, 360] {
            let pos = pet_position((800, 800, 180, 180), size, area);
            assert_eq!(pos.0 + size as i32 / 2, 890);
            assert_eq!(pos.1 + size as i32, 980);
            assert_eq!(pet_position((pos.0, pos.1, size, size), 180, area), (800, 800));
        }
        assert_eq!(pet_position((-1920, 0, 180, 180), 360, (-1920, 0, 1920, 1040)), (-1920, 0));
    }
    #[test]
    fn rejects_values_outside_supported_steps() {
        for value in [0, 74, 80, 201, u16::MAX] { assert!(validate(value).is_err()); }
        for value in [75, 100, 125, 150, 175, 200] { assert!(validate(value).is_ok()); }
    }
}
