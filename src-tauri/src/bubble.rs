use std::sync::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use crate::{domain::AppError, window_shell::ShellState};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content { revision: u64, text: String, reply: bool, left: bool }
pub struct BubbleState { ready: bool, content: Content }
impl BubbleState {
    pub fn new(quiet: bool) -> Self {
        Self { ready: false, content: Content { revision: 0, text: if quiet { "勿扰中" } else { "待机中" }.into(), reply: false, left: false } }
    }
}
fn failure(_: impl std::fmt::Display) -> AppError { AppError::new("bubble", "角色气泡更新失败，请重新打开桌宠。") }

// ATRI is fitted at 92% and centred in a square canvas. Use a model-specific
// head anchor calibrated against the rendered character, not transparent edges.
// Keep the anchor stable during animation instead of following moving vertices.
fn placement(pet: (i32,i32,u32,u32), desired: (u32,u32), area: (i32,i32,u32,u32), gap: i32) -> (i32,i32,u32,u32,bool) {
    let (x,y,w,h) = (pet.0 as i64,pet.1 as i64,pet.2 as i64,pet.3 as i64);
    let (ax,ay,aw,ah) = (area.0 as i64,area.1 as i64,area.2 as i64,area.3 as i64);
    let gap = gap as i64;
    let head_left = x+w*2/5;
    let head_right = x+w*3/5;
    let head_y = y+h/5;
    let right = (ax+aw-head_right-gap).max(0);
    let left = (head_left-gap-ax).max(0);
    let wanted = (desired.0 as i64).min(aw);
    let on_left = right < wanted && (left >= wanted || left > right);
    let space = if on_left { left } else { right };
    // A very narrow work area can have no usable side: keep the bubble on-screen.
    let width = wanted.min(space.max((64_i64).min(aw)));
    let height = (desired.1 as i64).min(ah);
    let bx = if on_left { head_left-gap-width } else { head_right+gap };
    // CSS tail tip is 20 logical pixels above the bubble window's bottom edge.
    let by = head_y-height+h/9;
    (bx.clamp(ax,ax+aw-width) as i32, by.clamp(ay,ay+ah-height) as i32,
        width as u32,height as u32,on_left)
}

fn content(revision: u64, resting: bool, quiet: bool, sending: bool, reply: Option<&str>) -> Content {
    let reply = reply.filter(|s| !s.trim().is_empty());
    let is_reply = !resting && !quiet && !sending && reply.is_some();
    let text = if resting { "休息中 · Zzz".into() }
        else if sending { "正在回复…".into() }
        else if quiet { "勿扰中".into() }
        else if let Some(reply) = reply { reply.chars().take(400).collect() }
        else { "待机中".into() };
    Content { revision, text, reply: is_reply, left: false }
}

pub fn sync(app: &AppHandle, revision: u64, resting: bool, quiet: bool, sending: bool, reply: Option<&str>) -> Result<(),AppError> {
    {
        let state = app.state::<Mutex<BubbleState>>();
        let mut state = state.lock().map_err(failure)?;
        if revision < state.content.revision { return Ok(()); }
        state.content = content(revision,resting,quiet,sending,reply);
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = refresh(&handle) { eprintln!("{}",error.message); }
    }).map_err(failure)
}

pub fn refresh(app: &AppHandle) -> Result<(),AppError> {
    let Some(bubble) = app.get_webview_window("bubble") else { return Ok(()); };
    let Some(pet) = app.get_webview_window("pet") else { return Ok(()); };
    let mut payload = {
        let state = app.state::<Mutex<BubbleState>>();
        let state = state.lock().map_err(failure)?;
        if !state.ready { return Ok(()); }
        state.content.clone()
    };
    if !pet.is_visible().map_err(failure)? || pet.is_minimized().map_err(failure)? {
        return bubble.hide().map_err(failure);
    }
    let scale = {
        let state = app.state::<Mutex<ShellState>>();
        let state = state.lock().map_err(failure)?;
        if state.scaling { return Ok(()); }
        state.scales.pet_percent as f64 / 100.0
    };
    let dpi = pet.scale_factor().map_err(failure)?;
    let monitor = pet.current_monitor().map_err(failure)?.ok_or_else(|| failure("monitor"))?;
    let area = monitor.work_area();
    let pos = pet.outer_position().map_err(failure)?;
    let size = pet.inner_size().map_err(failure)?;
    let logical = if payload.reply { (244.0,136.0) } else { (118.0,52.0) };
    let rect = placement((pos.x,pos.y,size.width,size.height),
        ((logical.0*scale*dpi).round() as u32,(logical.1*scale*dpi).round() as u32),
        (area.position.x,area.position.y,area.size.width,area.size.height),(4.0*scale*dpi).round() as i32);
    // set_zoom is relative to this WebView's DPI, which may differ across monitors.
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOCOPYBITS};
        let hwnd = bubble.hwnd().map_err(failure)?;
        if unsafe { SetWindowPos(hwnd.0 as _,std::ptr::null_mut(),rect.0,rect.1,rect.2 as i32,rect.3 as i32,
            SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOCOPYBITS) } == 0 { return Err(failure("SetWindowPos")); }
    }
    #[cfg(not(windows))]
    {
        bubble.set_size(tauri::PhysicalSize::new(rect.2,rect.3)).map_err(failure)?;
        bubble.set_position(tauri::PhysicalPosition::new(rect.0,rect.1)).map_err(failure)?;
    }
    bubble.set_zoom(scale*dpi/bubble.scale_factor().map_err(failure)?).map_err(failure)?;
    payload.left = rect.4;
    bubble.emit("bubble-content",payload).map_err(failure)?;
    bubble.show().map_err(failure)?;
    Ok(())
}

#[tauri::command]
pub async fn bubble_ready(window: WebviewWindow) -> Result<(),AppError> {
    if window.label() != "bubble" { return Err(AppError::new("permission","此窗口不是气泡窗口。")); }
    window.state::<Mutex<BubbleState>>().lock().map_err(failure)?.ready = true;
    let app = window.app_handle().clone();
    let (done,result) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || { let _ = done.send(refresh(&app)); }).map_err(failure)?;
    result.await.map_err(failure)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn right_then_left_and_top_edge() {
        assert_eq!(placement((800,500,180,180),(244,136),(0,0,1920,1040),4),(912,420,244,136,false));
        assert_eq!(placement((1740,0,180,180),(244,136),(0,0,1920,1040),4),(1564,0,244,136,true));
        assert_eq!(placement((-1920,0,180,180),(118,52),(-1920,0,1920,1040),4),(-1808,4,118,52,false));
    }
    #[test]
    fn narrow_space_and_scaled_sizes_stay_on_screen() {
        for scale in [0.75,1.0,1.5,2.0] {
            let size = (180.0*scale) as u32;
            for x in [0,100,300] {
                let (bx,by,w,h,_) = placement((x,0,size,size),((244.0*scale) as u32,(136.0*scale) as u32),(0,0,700,400),4);
                assert!(bx >= 0 && by >= 0 && bx as u32+w <= 700 && by as u32+h <= 400);
            }
        }
        assert_eq!(placement((140,80,180,180),(244,136),(0,0,440,380),4),(0,0,208,136,true));
    }
    #[test]
    fn bubble_tracks_head_anchor_at_every_user_scale() {
        for percent in [75,100,125,150,175,200] {
            let size = 180*percent/100;
            let height = 52*percent/100;
            let gap = (4*percent/100) as i32;
            let (x,y,_,_,left) = placement((800,500,size,size),(118*percent/100,height),(0,0,2400,1600),gap);
            assert!(!left);
            assert_eq!(x,800+(size*3/5) as i32+gap);
            assert_eq!(y+height as i32-(size/9) as i32,500+(size/5) as i32);
            assert!(x < 800+size as i32,"Bubble must use transparent space next to the head");
        }
    }
    #[test]
    fn state_priority_and_bounded_reply_without_provider() {
        assert_eq!(content(1,true,false,false,Some("reply")).text,"休息中 · Zzz");
        assert!(!content(2,false,true,false,Some("reply")).reply);
        assert_eq!(content(3,false,false,true,None).text,"正在回复…");
        let c = content(4,false,false,false,Some(&"好".repeat(500)));
        assert!(c.reply); assert_eq!(c.text.chars().count(),400);
        assert_eq!(content(5,false,false,false,None).text,"待机中");
    }
}
