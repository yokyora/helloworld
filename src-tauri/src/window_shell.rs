use crate::domain::AppError;
use serde::Deserialize;
use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode { Idle, Menu, Panel }

// Keep the character at the bottom centre; only move it when a screen edge
// would otherwise hide the expanded controls. All geometry here is physical.
fn anchored_position(old: (i32, i32, u32, u32), size: (u32, u32), area: (i32, i32, u32, u32)) -> (i32, i32) {
    let x = old.0 as i64 + (old.2 as i64 - size.0 as i64) / 2;
    let y = old.1 as i64 + old.3 as i64 - size.1 as i64;
    let max_x = area.0 as i64 + area.2.saturating_sub(size.0) as i64;
    let max_y = area.1 as i64 + area.3.saturating_sub(size.1) as i64;
    (x.clamp(area.0 as i64, max_x) as i32, y.clamp(area.1 as i64, max_y) as i32)
}

#[tauri::command]
pub async fn set_shell_mode(window: WebviewWindow, mode: ShellMode) -> Result<(), AppError> {
    if window.label() != "main" { return Err(AppError::new("permission", "此窗口不能改变桌宠布局。")); }
    let failure = |_| AppError::new("window", "窗口大小调整失败，请重试。 ");
    let scale = window.scale_factor().map_err(failure)?;
    let old_pos = window.outer_position().map_err(failure)?;
    let old_size = window.inner_size().map_err(failure)?;
    let monitor = window.current_monitor().map_err(failure)?
        .ok_or_else(|| AppError::new("window", "暂时无法确定桌宠所在屏幕。"))?;
    let area = monitor.work_area();
    let (width, height) = match mode { ShellMode::Idle => (180.0, 180.0), ShellMode::Menu => (300.0, 360.0), ShellMode::Panel => (440.0, 560.0) };
    let size = ((width * scale).round() as u32, (height * scale).round() as u32);
    let size = (size.0.min(area.size.width), size.1.min(area.size.height));
    let pos = anchored_position((old_pos.x, old_pos.y, old_size.width, old_size.height), size,
        (area.position.x, area.position.y, area.size.width, area.size.height));
    window.set_size(PhysicalSize::new(size.0, size.1)).map_err(failure)?;
    if let Err(error) = window.set_position(PhysicalPosition::new(pos.0, pos.1)) {
        let _ = window.set_size(old_size);
        return Err(failure(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::anchored_position;
    #[test]
    fn expanding_and_collapsing_preserve_character_anchor() {
        let area = (0, 0, 1920, 1040);
        let expanded = anchored_position((800, 800, 180, 180), (440, 560), area);
        assert_eq!(expanded, (670, 420));
        assert_eq!(anchored_position((expanded.0, expanded.1, 440, 560), (180, 180), area), (800, 800));
    }
    #[test]
    fn clamps_to_work_area_including_negative_monitor_coordinates() {
        assert_eq!(anchored_position((-1900, 20, 270, 270), (660, 840), (-1920, 0, 1920, 1040)), (-1920, 0));
        assert_eq!(anchored_position((1810, 980, 180, 180), (440, 560), (0, 0, 1920, 1040)), (1480, 480));
    }
}
